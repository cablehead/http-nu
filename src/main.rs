use std::io::Read;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use arc_swap::ArcSwap;
use clap::Parser;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::signal;
use tokio::sync::mpsc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use http_nu::{
    commands::{MjCommand, ResponseStartCommand, ReverseProxyCommand, StaticCommand, ToSse},
    handler::handle,
    listener::TlsConfig,
    Engine, Listener,
};

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Address to listen on [HOST]:PORT or <PATH> for Unix domain socket
    #[clap(value_parser)]
    addr: String,

    /// Path to PEM file containing certificate and private key
    #[clap(short, long)]
    tls: Option<PathBuf>,

    /// Nushell closure to handle requests, or '-' to read from stdin
    #[clap(value_parser)]
    closure: String,
}

fn add_commands(engine: &mut Engine) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    engine.add_commands(vec![
        Box::new(ResponseStartCommand::new()),
        Box::new(ReverseProxyCommand::new()),
        Box::new(StaticCommand::new()),
        Box::new(ToSse {}),
        Box::new(MjCommand::new()),
    ])
}

/// Spawns a dedicated OS thread that reads null-terminated scripts from stdin and sends parsed engines.
/// Uses blocking I/O to avoid async stdin issues with piped input.
fn spawn_stdin_reader(tx: mpsc::Sender<Engine>, interrupt: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        let mut buffer = Vec::new();
        let mut byte = [0u8; 1];
        let mut is_first = true;

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

            let script = String::from_utf8_lossy(&buffer);

            // Create new engine with updated script
            match Engine::new() {
                Ok(mut new_engine) => {
                    new_engine.set_signals(interrupt.clone());

                    if let Err(e) = add_commands(&mut new_engine) {
                        let err_str = e.to_string();
                        eprintln!("Failed to add commands: {err_str}");
                        if !is_first {
                            println!(
                                "{}",
                                serde_json::json!({
                                    "stamp": scru128::new(),
                                    "message": "script_update",
                                    "status": "error",
                                    "error": nu_utils::strip_ansi_string_likely(err_str)
                                })
                            );
                        }
                        continue;
                    }

                    match new_engine.parse_closure(&script) {
                        Ok(()) => {
                            let was_first = is_first;
                            is_first = false;
                            if tx.blocking_send(new_engine).is_err() {
                                break;
                            }
                            if !was_first {
                                println!(
                                    "{}",
                                    serde_json::json!({
                                        "stamp": scru128::new(),
                                        "message": "script_update",
                                        "status": "success"
                                    })
                                );
                            }
                        }
                        Err(e) => {
                            let err_str = e.to_string();
                            eprintln!("Script update failed: {err_str}");
                            if !is_first {
                                println!(
                                    "{}",
                                    serde_json::json!({
                                        "stamp": scru128::new(),
                                        "message": "script_update",
                                        "status": "error",
                                        "error": nu_utils::strip_ansi_string_likely(err_str)
                                    })
                                );
                            }
                        }
                    }
                }
                Err(e) => {
                    let err_str = e.to_string();
                    eprintln!("Failed to create new engine: {err_str}");
                    if !is_first {
                        println!(
                            "{}",
                            serde_json::json!({
                                "stamp": scru128::new(),
                                "message": "script_update",
                                "status": "error",
                                "error": nu_utils::strip_ansi_string_likely(err_str)
                            })
                        );
                    }
                }
            }
        }
    });
}

async fn serve(
    args: Args,
    mut rx: mpsc::Receiver<Engine>,
    interrupt: Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Wait for the first engine
    let first_engine = rx
        .recv()
        .await
        .expect("no engine received - stdin closed without providing a script");

    // Set up Ctrl-C handler with the first engine's state
    setup_ctrlc_handler(&first_engine, interrupt.clone())?;

    let engine = Arc::new(ArcSwap::from_pointee(first_engine));

    // Spawn task to receive and swap in new engines
    let engine_updater = engine.clone();
    tokio::spawn(async move {
        while let Some(new_engine) = rx.recv().await {
            engine_updater.store(Arc::new(new_engine));
        }
    });

    // Configure TLS if enabled
    let tls_config = if let Some(pem_path) = args.tls {
        Some(TlsConfig::from_pem(pem_path)?)
    } else {
        None
    };

    let mut listener = Listener::bind(&args.addr, tls_config).await?;
    println!(
        "{}",
        serde_json::json!({"stamp": scru128::new(), "message": "start", "address": format!("{}", listener)})
    );

    let shutdown = shutdown_signal(interrupt.clone());
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, remote_addr)) => {
                        let io = TokioIo::new(stream);
                        let engine = engine.clone();

                        tokio::task::spawn(async move {
                            let service = service_fn(move |req| {
                                handle(engine.clone(), remote_addr, req)
                            });
                            if let Err(err) = hyper::server::conn::http1::Builder::new()
                                .serve_connection(io, service)
                                .await
                            {
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
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "http_nu=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("failed to install default rustls CryptoProvider");

    // Initialize nu_command's TLS crypto provider
    nu_command::tls::CRYPTO_PROVIDER
        .default()
        .then_some(())
        .expect("failed to set nu_command crypto provider");

    let args = Args::parse();
    let read_stdin = args.closure == "-";

    // Create channel for engines
    let (tx, rx) = mpsc::channel::<Engine>(1);

    // Set up interrupt signal
    let interrupt = Arc::new(AtomicBool::new(false));

    if read_stdin {
        // Spawn dedicated stdin reader thread
        spawn_stdin_reader(tx, interrupt.clone());
    } else {
        // Parse the closure and send a single engine
        let mut engine = Engine::new()?;
        engine.set_signals(interrupt.clone());
        add_commands(&mut engine)?;

        if let Err(e) = engine.parse_closure(&args.closure) {
            eprintln!("{e}");
            std::process::exit(1);
        }

        tx.send(engine).await.expect("channel closed unexpectedly");
        drop(tx); // Close the channel
    }

    serve(args, rx, interrupt).await
}
