use std::io::Read;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use arc_swap::ArcSwap;
use clap::Parser;
use hyper::service::service_fn;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder as HttpConnectionBuilder;
use hyper_util::server::graceful::GracefulShutdown;
use tokio::signal;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use http_nu::{
    engine::script_to_engine,
    handler::handle,
    listener::TlsConfig,
    logging::{HumanLayer, JsonlLayer},
    Engine, Listener,
};

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

    /// Nushell closure to handle requests, or '-' to read from stdin
    #[clap(value_parser)]
    closure: Option<String>,

    /// Log format: human (live-updating) or jsonl (structured)
    #[clap(long, default_value = "human")]
    log_format: LogFormat,

    /// Trust proxies from these CIDR ranges for X-Forwarded-For parsing
    #[clap(long = "trust-proxy", value_name = "CIDR")]
    trust_proxies: Vec<ipnet::IpNet>,
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
) -> Result<Engine, Box<dyn std::error::Error + Send + Sync>> {
    let mut engine = Engine::new()?;
    engine.add_custom_commands()?;

    // Load plugins
    for plugin_path in plugins {
        engine.load_plugin(plugin_path)?;
    }

    engine.set_signals(interrupt.clone());
    setup_ctrlc_handler(&engine, interrupt)?;
    Ok(engine)
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

async fn serve(
    addr: String,
    tls: Option<PathBuf>,
    base_engine: Engine,
    mut rx: mpsc::Receiver<String>,
    interrupt: Arc<AtomicBool>,
    trusted_proxies: Vec<ipnet::IpNet>,
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
                engine_updater.store(Arc::new(new_engine));
                println!(
                    "{}",
                    serde_json::json!({
                        "stamp": scru128::new(),
                        "message": "reload"
                    })
                );
            }
        }
    });

    // Configure TLS if enabled
    let tls_config = if let Some(pem_path) = tls {
        Some(TlsConfig::from_pem(pem_path)?)
    } else {
        None
    };

    let mut listener = Listener::bind(&addr, tls_config).await?;
    println!(
        "{}",
        serde_json::json!({"stamp": scru128::new(), "message": "start", "address": format!("{}", listener)})
    );

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
    if inflight > 0 {
        println!(
            "{}",
            serde_json::json!({
                "stamp": scru128::new(),
                "message": "shutdown",
                "inflight": inflight
            })
        );

        tokio::select! {
            _ = graceful.shutdown() => {
                println!(
                    "{}",
                    serde_json::json!({"stamp": scru128::new(), "message": "shutdown_complete"})
                );
            }
            _ = tokio::time::sleep(Duration::from_secs(10)) => {
                println!(
                    "{}",
                    serde_json::json!({"stamp": scru128::new(), "message": "shutdown_timeout"})
                );
            }
        }
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

    // Set up tracing based on log format
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "http_nu=info".into());

    match args.log_format {
        LogFormat::Human => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(HumanLayer::new())
                .init();
        }
        LogFormat::Jsonl => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(JsonlLayer::new())
                .init();
        }
    }

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

        for plugin_path in &args.plugins {
            engine.load_plugin(plugin_path)?;
        }

        engine.set_signals(interrupt.clone());

        match engine.eval(&script) {
            Ok(value) => {
                println!("{}", value.to_expanded_string(" ", &engine.state.config));
                return Ok(());
            }
            Err(e) => {
                eprintln!("{e}");
                std::process::exit(1);
            }
        }
    }

    // Server mode (default)
    let addr = args.addr.expect("addr required for server mode");
    let closure = args.closure.expect("closure required for server mode");
    let read_stdin = closure == "-";

    // Create base engine with commands, signals, and plugins
    let base_engine = create_base_engine(interrupt.clone(), &args.plugins)?;

    // Create channel for scripts
    let (tx, rx) = mpsc::channel::<String>(1);

    if read_stdin {
        // Spawn dedicated stdin reader thread
        spawn_stdin_reader(tx);
    } else {
        // Send the closure as a script
        tx.send(closure).await.expect("channel closed unexpectedly");
        drop(tx); // Close the channel
    }

    serve(
        addr,
        args.tls,
        base_engine,
        rx,
        interrupt,
        args.trust_proxies,
    )
    .await
}
