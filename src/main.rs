use std::io::Read;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use arc_swap::ArcSwap;
use clap::Parser;
use http_nu::{
    engine::script_to_engine,
    handler::handle,
    listener::TlsConfig,
    logging::{
        init_broadcast, log_reloaded, log_started, log_stop_timed_out, log_stopped, log_stopping,
        run_human_handler, run_jsonl_handler, shutdown, StartupOptions,
    },
    Engine, Listener,
};
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as HttpConnectionBuilder;
use hyper_util::server::graceful::GracefulShutdown;
use notify::{RecursiveMode, Watcher};
use tokio::signal;
use tokio::sync::mpsc;

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    /// Address to listen on [HOST]:PORT or <PATH> for Unix domain socket
    #[clap(value_parser)]
    addr: Option<String>,

    /// Path to PEM file containing certificate and private key
    #[clap(short, long)]
    tls: Option<PathBuf>,

    /// Load a Nushell plugin from the specified path (can be used multiple times)
    #[clap(long = "plugin", global = true, value_parser)]
    plugins: Vec<PathBuf>,

    /// Script file to run, or '-' to read from stdin
    #[clap(value_parser)]
    script: Option<String>,

    /// Run script from command line instead of file
    #[clap(short = 'c', long = "commands", conflicts_with = "watch")]
    commands: Option<String>,

    /// Watch for script changes and reload automatically.
    /// For file scripts: watches the script's directory for any changes.
    /// For stdin (-): reads null-terminated scripts for hot reload.
    #[clap(short = 'w', long = "watch")]
    watch: bool,

    /// Log format: human (live-updating) or jsonl (structured)
    #[clap(long, default_value = "human")]
    log_format: LogFormat,

    /// Path to store directory (enables .cat, .append, .cas commands)
    #[clap(long, help_heading = "cross.stream")]
    store: Option<PathBuf>,

    /// Enable handlers, generators, and commands
    #[clap(long, requires = "store", help_heading = "cross.stream")]
    services: bool,

    /// Expose API on additional address ([HOST]:PORT or iroh://)
    #[clap(
        long,
        requires = "store",
        value_name = "ADDR",
        help_heading = "cross.stream"
    )]
    expose: Option<String>,

    /// Trust proxies from these CIDR ranges for X-Forwarded-For parsing
    #[clap(long = "trust-proxy", value_name = "CIDR")]
    trust_proxies: Vec<ipnet::IpNet>,

    /// Set NU_LIB_DIRS for module resolution (can be repeated)
    #[clap(short = 'I', long = "include-path", global = true, value_name = "PATH")]
    include_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum LogFormat {
    #[default]
    Human,
    Jsonl,
}

#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Evaluate a Nushell script with http-nu commands and exit
    Eval {
        /// Script file to evaluate, or '-' to read from stdin
        #[clap(value_parser)]
        file: Option<String>,

        /// Evaluate script from command line
        #[clap(short = 'c', long = "commands")]
        commands: Option<String>,
    },
}

/// Creates and configures the base engine with all commands, signals, and ctrlc handler.
#[cfg(feature = "cross-stream")]
fn create_base_engine(
    interrupt: Arc<AtomicBool>,
    plugins: &[PathBuf],
    include_paths: &[PathBuf],
    store: Option<&xs::store::Store>,
) -> Result<Engine, Box<dyn std::error::Error + Send + Sync>> {
    let mut engine = Engine::new()?;
    engine.add_custom_commands()?;
    engine.set_lib_dirs(include_paths)?;

    // Load plugins
    for plugin_path in plugins {
        engine.load_plugin(plugin_path)?;
    }

    // Add cross.stream commands if store is enabled
    if let Some(store) = store {
        engine.add_store_commands(store)?;
    }

    engine.set_signals(interrupt.clone());
    setup_ctrlc_handler(&engine, interrupt)?;
    Ok(engine)
}

#[cfg(not(feature = "cross-stream"))]
fn create_base_engine(
    interrupt: Arc<AtomicBool>,
    plugins: &[PathBuf],
    include_paths: &[PathBuf],
) -> Result<Engine, Box<dyn std::error::Error + Send + Sync>> {
    let mut engine = Engine::new()?;
    engine.add_custom_commands()?;
    engine.set_lib_dirs(include_paths)?;

    for plugin_path in plugins {
        engine.load_plugin(plugin_path)?;
    }

    engine.set_signals(interrupt.clone());
    setup_ctrlc_handler(&engine, interrupt)?;
    Ok(engine)
}

/// Spawns a file watcher that watches the script's directory for any changes.
/// When a change is detected, re-reads the script file and sends it.
fn spawn_file_watcher(script_path: PathBuf, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let watch_dir = script_path.parent().unwrap_or(&script_path).to_path_buf();

        let (raw_tx, raw_rx) = std::sync::mpsc::channel();

        let mut watcher = notify::recommended_watcher(raw_tx).expect("Failed to create watcher");

        watcher
            .watch(&watch_dir, RecursiveMode::Recursive)
            .expect("Failed to watch directory");

        let debounce = Duration::from_millis(100);
        let mut pending_reload = false;

        loop {
            let timeout = if pending_reload {
                debounce
            } else {
                Duration::from_secs(86400)
            };

            match raw_rx.recv_timeout(timeout) {
                Ok(Ok(event)) => {
                    use notify::EventKind;
                    let dominated_by = matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
                    );
                    if dominated_by {
                        pending_reload = true;
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("Watch error: {e:?}");
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if pending_reload {
                        pending_reload = false;
                        match std::fs::read_to_string(&script_path) {
                            Ok(content) => {
                                if tx.blocking_send(content).is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                eprintln!("Error reading script file: {e}");
                            }
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    break;
                }
            }
        }
    });
}

/// Spawns a dedicated OS thread that reads null-terminated scripts from stdin and sends them.
/// Uses blocking I/O to avoid async stdin issues with piped input.
fn spawn_stdin_reader(tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buffer = Vec::new();
        let mut byte = [0u8; 1];

        loop {
            buffer.clear();

            // Read until null terminator or EOF
            loop {
                match stdin.read(&mut byte) {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        if byte[0] == b'\0' {
                            break;
                        }
                        buffer.push(byte[0]);
                    }
                    Err(e) => {
                        eprintln!("Error reading stdin: {e}");
                        return;
                    }
                }
            }

            if buffer.is_empty() {
                break;
            }

            let script = String::from_utf8_lossy(&buffer).into_owned();

            if tx.blocking_send(script).is_err() {
                break;
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
async fn serve(
    addr: String,
    tls: Option<PathBuf>,
    base_engine: Engine,
    mut rx: mpsc::Receiver<String>,
    interrupt: Arc<AtomicBool>,
    trusted_proxies: Vec<ipnet::IpNet>,
    start_time: std::time::Instant,
    startup_options: StartupOptions,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Wait for a valid first script (loop to handle parse errors)
    let first_engine = loop {
        let script = rx
            .recv()
            .await
            .expect("no script received - stdin closed without providing a valid script");

        if let Some(engine) = script_to_engine(&base_engine, &script) {
            break engine;
        }
        // script_to_engine already logged the error, continue waiting
    };

    let engine = Arc::new(ArcSwap::from_pointee(first_engine));

    // Spawn task to receive scripts and swap in new engines
    let engine_updater = engine.clone();
    let base_for_updates = base_engine;
    tokio::spawn(async move {
        while let Some(script) = rx.recv().await {
            if let Some(new_engine) = script_to_engine(&base_for_updates, &script) {
                // Signal reload to cancel SSE streams on old engine
                engine_updater.load().reload_token.cancel();
                engine_updater.store(Arc::new(new_engine));
                log_reloaded();
            }
        }
    });

    // Configure TLS if enabled
    let tls_config = if let Some(pem_path) = tls {
        Some(TlsConfig::from_pem(pem_path)?)
    } else {
        None
    };

    let tls_enabled = tls_config.is_some();
    let mut listener = Listener::bind(&addr, tls_config).await?;
    let startup_ms = start_time.elapsed().as_millis();
    let addr_display = {
        let raw = format!("{listener}");
        // Format TCP addresses as clickable URLs, leave Unix sockets as-is
        if raw.starts_with('/') {
            raw
        } else {
            // Strip " (TLS)" suffix from Listener's Display
            let addr = raw.strip_suffix(" (TLS)").unwrap_or(&raw);
            if tls_enabled {
                format!("https://{addr}")
            } else {
                format!("http://{addr}")
            }
        }
    };
    log_started(&addr_display, startup_ms, startup_options);

    // HTTP/1 + HTTP/2 auto-detection builder
    let http_builder = HttpConnectionBuilder::new(TokioExecutor::new());

    // Graceful shutdown tracker for all connections
    let graceful = GracefulShutdown::new();

    // Wrap trusted_proxies in Arc for sharing across connections
    let trusted_proxies = Arc::new(trusted_proxies);

    let shutdown = shutdown_signal(interrupt.clone());
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, remote_addr)) => {
                        let io = TokioIo::new(stream);
                        let engine = engine.clone();
                        let trusted_proxies = trusted_proxies.clone();

                        let service = service_fn(move |req| {
                            handle(engine.clone(), remote_addr, trusted_proxies.clone(), req)
                        });

                        // serve_connection_with_upgrades supports HTTP/1 and HTTP/2
                        let conn = http_builder.serve_connection_with_upgrades(io, service);

                        // Watch this connection for graceful shutdown
                        let conn = graceful.watch(conn.into_owned());

                        tokio::task::spawn(async move {
                            if let Err(err) = conn.await {
                                // Suppress errors normal for client disconnect
                                if let Some(hyper_err) = err.downcast_ref::<hyper::Error>() {
                                    if hyper_err.is_incomplete_message()
                                        || hyper_err.is_body_write_aborted()
                                    {
                                        return;
                                    }
                                }
                                eprintln!("Connection error: {err}");
                            }
                        });
                    }
                    Err(err) => {
                        eprintln!("Error accepting connection: {err}");
                        continue;
                    }
                }
            }
            _ = &mut shutdown => {
                break;
            }
        }
    }

    // Graceful shutdown: wait for inflight connections to complete
    let inflight = graceful.count();
    let mut timed_out = false;

    if inflight > 0 {
        log_stopping(inflight);

        tokio::select! {
            _ = graceful.shutdown() => {}
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                timed_out = true;
            }
        }
    }

    if timed_out {
        log_stop_timed_out();
    } else {
        log_stopped();
    }

    Ok(())
}

async fn shutdown_signal(interrupt: Arc<AtomicBool>) {
    use tokio::time::{interval, Duration};

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    let interrupt_check = async {
        let mut interval = interval(Duration::from_millis(100));
        loop {
            interval.tick().await;
            if interrupt.load(Ordering::Relaxed) {
                break;
            }
        }
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
        _ = interrupt_check => {},
    }
}

/// Sets up Ctrl-C handling
fn setup_ctrlc_handler(
    engine: &Engine,
    interrupt: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    ctrlc::set_handler({
        let interrupt = interrupt.clone();
        let engine_state = engine.state.clone();
        move || {
            interrupt.store(true, Ordering::Relaxed);
            // Kill all active jobs
            if let Ok(mut jobs) = engine_state.jobs.lock() {
                let job_ids: Vec<_> = jobs.iter().map(|(id, _)| id).collect();
                for id in job_ids {
                    let _ = jobs.kill_and_remove(id);
                }
            }
        }
    })?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    // Set up logging handler based on log format (both spawn dedicated threads)
    let rx = init_broadcast();
    let log_handle = match args.log_format {
        LogFormat::Human => run_human_handler(rx),
        LogFormat::Jsonl => run_jsonl_handler(rx),
    };

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install default rustls CryptoProvider");

    // Initialize nu_command's TLS crypto provider
    nu_command::tls::CRYPTO_PROVIDER
        .default()
        .then_some(())
        .expect("failed to set nu_command crypto provider");

    // Set up interrupt signal
    let interrupt = Arc::new(AtomicBool::new(false));

    // Handle subcommands
    if let Some(Command::Eval { file, commands }) = args.command {
        let script = match (&file, &commands) {
            (Some(_), Some(_)) => {
                eprintln!("Error: cannot specify both file and --commands");
                std::process::exit(1);
            }
            (None, None) => {
                eprintln!("Error: provide a file or use --commands");
                std::process::exit(1);
            }
            (Some(path), None) if path == "-" => {
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            }
            (Some(path), None) => std::fs::read_to_string(path)?,
            (None, Some(cmd)) => cmd.clone(),
        };

        let mut engine = Engine::new()?;
        engine.add_custom_commands()?;
        engine.set_lib_dirs(&args.include_paths)?;

        for plugin_path in &args.plugins {
            engine.load_plugin(plugin_path)?;
        }

        engine.set_signals(interrupt.clone());

        let exit_code = match engine.eval(&script) {
            Ok(value) => {
                let output = value.to_expanded_string(" ", &engine.state.config);
                if !output.is_empty() {
                    println!("{output}");
                }
                0
            }
            Err(e) => {
                eprintln!("{e}");
                1
            }
        };
        shutdown();
        log_handle.join().ok();
        std::process::exit(exit_code);
    }

    // Server mode (default)
    let Some(addr) = args.addr else {
        eprintln!("Usage: http-nu <ADDR> [OPTIONS]");
        eprintln!("       http-nu eval [OPTIONS]");
        eprintln!("\nRun `http-nu --help` for more information.");
        std::process::exit(1);
    };

    // Create cross.stream store if --store is specified
    #[cfg(feature = "cross-stream")]
    let store = args
        .store
        .as_ref()
        .map(|p| xs::store::Store::new(p.clone()));

    // Spawn xs API server if store is enabled
    #[cfg(feature = "cross-stream")]
    if let Some(ref store) = store {
        let store_for_api = store.clone();
        let expose = args.expose.clone();
        tokio::spawn(async move {
            let engine = xs::nu::Engine::new().expect("Failed to create xs nu::Engine");
            if let Err(e) = xs::api::serve(store_for_api, engine, expose).await {
                eprintln!("Store API server error: {e}");
            }
        });

        // Spawn xs services (handlers, generators, commands) if --services is set
        if args.services {
            let store_for_handlers = store.clone();
            tokio::spawn(async move {
                let engine = xs::nu::Engine::new().expect("Failed to create xs nu::Engine");
                if let Err(e) = xs::handlers::serve(store_for_handlers, engine).await {
                    eprintln!("Handlers serve error: {e}");
                }
            });

            let store_for_generators = store.clone();
            tokio::spawn(async move {
                let engine = xs::nu::Engine::new().expect("Failed to create xs nu::Engine");
                if let Err(e) = xs::generators::serve(store_for_generators, engine).await {
                    eprintln!("Generators serve error: {e}");
                }
            });

            let store_for_commands = store.clone();
            tokio::spawn(async move {
                let engine = xs::nu::Engine::new().expect("Failed to create xs nu::Engine");
                if let Err(e) = xs::commands::serve(store_for_commands, engine).await {
                    eprintln!("Commands serve error: {e}");
                }
            });
        }
    }

    // Create base engine with commands, signals, and plugins
    #[cfg(feature = "cross-stream")]
    let base_engine = create_base_engine(
        interrupt.clone(),
        &args.plugins,
        &args.include_paths,
        store.as_ref(),
    )?;

    #[cfg(not(feature = "cross-stream"))]
    let base_engine = create_base_engine(interrupt.clone(), &args.plugins, &args.include_paths)?;

    // Create channel for scripts
    let (tx, rx) = mpsc::channel::<String>(1);

    // Determine script source and set up appropriate watcher/reader
    match (&args.script, &args.commands, args.watch) {
        (Some(_), Some(_), _) => {
            eprintln!("Error: cannot specify both script file and --commands");
            std::process::exit(1);
        }
        (None, None, _) => {
            eprintln!("Error: provide a script file or use --commands");
            std::process::exit(1);
        }
        // -c flag: use command content directly (conflicts_with prevents -w)
        (None, Some(cmd), false) => {
            tx.send(cmd.clone())
                .await
                .expect("channel closed unexpectedly");
            drop(tx);
        }
        // stdin without -w: read once
        (Some(path), None, false) if path == "-" => {
            let mut content = String::new();
            std::io::stdin()
                .read_to_string(&mut content)
                .expect("Failed to read from stdin");
            tx.send(content).await.expect("channel closed unexpectedly");
            drop(tx);
        }
        // stdin with -w: spawn stdin reader for null-terminated scripts
        (Some(path), None, true) if path == "-" => {
            spawn_stdin_reader(tx);
        }
        // file without -w: read once
        (Some(path), None, false) => {
            let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
                eprintln!("Error reading {path}: {e}");
                std::process::exit(1);
            });
            tx.send(content).await.expect("channel closed unexpectedly");
            drop(tx);
        }
        // file with -w: read initial content and spawn file watcher
        (Some(path), None, true) => {
            let script_path = PathBuf::from(path);
            let content = std::fs::read_to_string(&script_path).unwrap_or_else(|e| {
                eprintln!("Error reading {path}: {e}");
                std::process::exit(1);
            });
            tx.send(content).await.expect("channel closed unexpectedly");
            spawn_file_watcher(script_path, tx);
        }
        // -c with -w is prevented by clap conflicts_with
        (None, Some(_), true) => unreachable!(),
    }

    let startup_options = StartupOptions {
        watch: args.watch,
        tls: args.tls.as_ref().map(|p| p.display().to_string()),
        store: args.store.as_ref().map(|p| p.display().to_string()),
        expose: args.expose.clone(),
        services: args.services,
    };

    serve(
        addr,
        args.tls,
        base_engine,
        rx,
        interrupt,
        args.trust_proxies,
        std::time::Instant::now(),
        startup_options,
    )
    .await?;

    shutdown();
    log_handle.join().ok();
    Ok(())
}
