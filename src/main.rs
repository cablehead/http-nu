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
    engine::{script_to_engine, HttpNuOptions},
    handler::{handle, AppConfig},
    listener::TlsConfig,
    logging::{
        init_broadcast, log_reloaded, log_started, log_stop_timed_out, log_stopped, log_stopping,
        run_human_handler, run_jsonl_handler, shutdown, StartupOptions,
    },
    store::Store,
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
    #[clap(short = 'c', long = "commands", conflicts_with_all = ["watch", "script"])]
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
    #[cfg(feature = "cross-stream")]
    #[clap(long, help_heading = "cross.stream")]
    store: Option<PathBuf>,

    /// Enable actors, services, and actions
    #[cfg(feature = "cross-stream")]
    #[clap(long, requires = "store", help_heading = "cross.stream")]
    services: bool,

    /// Load handler closure from a store topic (use with -w to live-reload on changes)
    #[cfg(feature = "cross-stream")]
    #[clap(
        long,
        requires = "store",
        conflicts_with_all = ["script", "commands"],
        value_name = "TOPIC",
        help_heading = "cross.stream"
    )]
    topic: Option<String>,

    /// Expose API on additional address ([HOST]:PORT or iroh://)
    #[cfg(feature = "cross-stream")]
    #[clap(
        long,
        requires = "store",
        value_name = "ADDR",
        help_heading = "cross.stream"
    )]
    expose: Option<String>,

    /// Development mode: relaxes security defaults (e.g. omits Secure flag on cookies)
    #[clap(long, global = true)]
    dev: bool,

    /// Serve the embedded Datastar JS bundle at /datastar@<version>.js
    #[clap(long)]
    datastar: bool,

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
fn create_base_engine(
    interrupt: Arc<AtomicBool>,
    plugins: &[PathBuf],
    include_paths: &[PathBuf],
    store: Option<&Store>,
    options: &HttpNuOptions,
) -> Result<Engine, Box<dyn std::error::Error + Send + Sync>> {
    let mut engine = Engine::new()?;
    engine.add_custom_commands()?;
    engine.set_lib_dirs(include_paths)?;
    engine.set_http_nu_const(options)?;

    for plugin_path in plugins {
        engine.load_plugin(plugin_path)?;
    }

    if let Some(store) = store {
        store.configure_engine(&mut engine)?;
    }

    engine.set_signals(interrupt.clone());
    setup_ctrlc_handler(&engine, interrupt)?;
    Ok(engine)
}

/// Read script from file, convert to engine, send through `tx`. If `watch` is true,
/// spawn a watcher that re-reads, converts, and sends on changes.
async fn file_source(path: &str, watch: bool, base_engine: Engine, tx: mpsc::Sender<Engine>) {
    let content = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Error reading {path}: {e}");
        std::process::exit(1);
    });

    let script_path = PathBuf::from(path).canonicalize().unwrap_or_else(|e| {
        eprintln!("Error resolving {path}: {e}");
        std::process::exit(1);
    });

    if let Some(engine) = script_to_engine(&base_engine, &content, Some(&script_path)) {
        tx.send(engine).await.expect("channel closed unexpectedly");
    }

    if watch {
        std::thread::spawn(move || {
            let watch_dir = script_path.parent().unwrap_or(&script_path).to_path_buf();

            let (raw_tx, raw_rx) = std::sync::mpsc::channel();

            let mut watcher =
                notify::recommended_watcher(raw_tx).expect("Failed to create watcher");

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
                                    if let Some(engine) =
                                        script_to_engine(&base_engine, &content, Some(&script_path))
                                    {
                                        if tx.blocking_send(engine).is_err() {
                                            break;
                                        }
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
}

/// Read script from stdin, convert to engine, send through `tx`. If `watch` is true,
/// spawn a reader that reads null-terminated scripts for hot reload.
async fn stdin_source(watch: bool, base_engine: Engine, tx: mpsc::Sender<Engine>) {
    if watch {
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

                if let Some(engine) = script_to_engine(&base_engine, &script, None) {
                    if tx.blocking_send(engine).is_err() {
                        break;
                    }
                }
            }
        });
    } else {
        let mut content = String::new();
        std::io::stdin()
            .read_to_string(&mut content)
            .expect("Failed to read from stdin");
        if let Some(engine) = script_to_engine(&base_engine, &content, None) {
            tx.send(engine).await.expect("channel closed unexpectedly");
        }
    }
}

async fn serve(
    addr: String,
    tls: Option<PathBuf>,
    mut rx: mpsc::Receiver<Engine>,
    interrupt: Arc<AtomicBool>,
    config: AppConfig,
    start_time: std::time::Instant,
    startup_options: StartupOptions,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Wait for first valid engine from source
    let first_engine = rx
        .recv()
        .await
        .expect("no engine received - source closed without providing a valid engine");

    let engine = Arc::new(ArcSwap::from_pointee(first_engine));

    // Spawn task to receive engines and swap in new ones
    let engine_updater = engine.clone();
    tokio::spawn(async move {
        while let Some(new_engine) = rx.recv().await {
            // Signal reload to cancel SSE streams on old engine
            engine_updater.load().reload_token.cancel();
            engine_updater.store(Arc::new(new_engine));
            log_reloaded();
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

    // Wrap config in Arc for sharing across connections
    let config = Arc::new(config);

    let shutdown = shutdown_signal(interrupt.clone());
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, remote_addr)) => {
                        let io = TokioIo::new(stream);
                        let engine = engine.clone();
                        let config = config.clone();

                        let service = service_fn(move |req| {
                            handle(engine.clone(), remote_addr, config.clone(), req)
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
        let (script, script_path) = match (&file, &commands) {
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
                (buf, None)
            }
            (Some(path), None) => {
                let p = PathBuf::from(path).canonicalize()?;
                (std::fs::read_to_string(&p)?, Some(p))
            }
            (None, Some(cmd)) => (cmd.clone(), None),
        };

        let mut engine = Engine::new()?;
        engine.add_custom_commands()?;
        engine.set_lib_dirs(&args.include_paths)?;
        engine.set_http_nu_const(&HttpNuOptions {
            dev: args.dev,
            ..Default::default()
        })?;

        for plugin_path in &args.plugins {
            engine.load_plugin(plugin_path)?;
        }

        engine.set_signals(interrupt.clone());

        let exit_code = match engine.eval(&script, script_path.as_deref()) {
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

    // Create channel for engines
    let (tx, rx) = mpsc::channel::<Engine>(1);

    // Create cross.stream store if --store is specified
    #[cfg(feature = "cross-stream")]
    let store = match args.store {
        Some(ref path) => {
            match Store::init(path.clone(), args.services, args.expose.clone()).await {
                Ok(store) => Some(store),
                Err(e) => {
                    eprintln!("Failed to open store at {}: {e}", path.display());
                    std::process::exit(1);
                }
            }
        }
        None => None,
    };
    #[cfg(not(feature = "cross-stream"))]
    let store: Option<Store> = None;

    // Build $HTTP_NU options from CLI args
    let http_nu_options = HttpNuOptions {
        dev: args.dev,
        datastar: args.datastar,
        watch: args.watch,
        tls: args.tls.as_ref().map(|p| p.display().to_string()),
        #[cfg(feature = "cross-stream")]
        store: args.store.as_ref().map(|p| p.display().to_string()),
        #[cfg(not(feature = "cross-stream"))]
        store: None,
        #[cfg(feature = "cross-stream")]
        topic: args.topic.clone(),
        #[cfg(not(feature = "cross-stream"))]
        topic: None,
        #[cfg(feature = "cross-stream")]
        expose: args.expose.clone(),
        #[cfg(not(feature = "cross-stream"))]
        expose: None,
        #[cfg(feature = "cross-stream")]
        services: args.services,
        #[cfg(not(feature = "cross-stream"))]
        services: false,
    };

    // Create base engine with commands, signals, and plugins
    let base_engine = create_base_engine(
        interrupt.clone(),
        &args.plugins,
        &args.include_paths,
        store.as_ref(),
        &http_nu_options,
    )?;

    // Source: --topic (direct store read, with optional watch for live-reload)
    #[cfg(feature = "cross-stream")]
    let tx = if let (Some(ref topic), Some(ref store)) = (&args.topic, &store) {
        store
            .topic_source(topic, args.watch, base_engine.clone(), tx)
            .await;
        None
    } else {
        Some(tx)
    };
    #[cfg(not(feature = "cross-stream"))]
    let tx = Some(tx);

    // Source: file, stdin, or --commands
    if let Some(tx) = tx {
        match (&args.script, &args.commands) {
            (Some(_), Some(_)) => unreachable!(), // clap conflicts_with
            (None, Some(_)) if args.watch => unreachable!(), // clap conflicts_with
            (None, None) => {
                eprintln!("Error: provide a script file, --commands, or --topic");
                std::process::exit(1);
            }
            (None, Some(cmd)) => {
                if let Some(engine) = script_to_engine(&base_engine, cmd, None) {
                    tx.send(engine).await.expect("channel closed unexpectedly");
                }
            }
            (Some(path), None) if path == "-" => {
                stdin_source(args.watch, base_engine.clone(), tx).await;
            }
            (Some(path), None) => {
                file_source(path, args.watch, base_engine.clone(), tx).await;
            }
        }
    }

    let startup_options = StartupOptions {
        watch: args.watch,
        tls: args.tls.as_ref().map(|p| p.display().to_string()),
        #[cfg(feature = "cross-stream")]
        store: args.store.as_ref().map(|p| p.display().to_string()),
        #[cfg(not(feature = "cross-stream"))]
        store: None,
        #[cfg(feature = "cross-stream")]
        topic: args.topic.clone(),
        #[cfg(not(feature = "cross-stream"))]
        topic: None,
        #[cfg(feature = "cross-stream")]
        expose: args.expose.clone(),
        #[cfg(not(feature = "cross-stream"))]
        expose: None,
        #[cfg(feature = "cross-stream")]
        services: args.services,
        #[cfg(not(feature = "cross-stream"))]
        services: false,
    };

    serve(
        addr,
        args.tls,
        rx,
        interrupt,
        AppConfig {
            trusted_proxies: args.trust_proxies,
            datastar: args.datastar,
            dev: args.dev,
        },
        std::time::Instant::now(),
        startup_options,
    )
    .await?;

    shutdown();
    log_handle.join().ok();
    Ok(())
}
