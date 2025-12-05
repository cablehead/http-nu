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

use http_nu::{engine::script_to_engine, handler::handle, listener::TlsConfig, Engine, Listener};

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

/// Creates and configures the base engine with all commands, signals, and ctrlc handler.
fn create_base_engine(
    interrupt: Arc<AtomicBool>,
) -> Result<Engine, Box<dyn std::error::Error + Send + Sync>> {
    let mut engine = Engine::new()?;
    engine.add_custom_commands()?;
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
    args: Args,
    base_engine: Engine,
    mut rx: mpsc::Receiver<String>,
    interrupt: Arc<AtomicBool>,
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
            }
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

    // Set up interrupt signal
    let interrupt = Arc::new(AtomicBool::new(false));

    // Create base engine with commands and signals
    let base_engine = create_base_engine(interrupt.clone())?;

    // Create channel for scripts
    let (tx, rx) = mpsc::channel::<String>(1);

    if read_stdin {
        // Spawn dedicated stdin reader thread
        spawn_stdin_reader(tx);
    } else {
        // Send the closure as a script
        tx.send(args.closure.clone())
            .await
            .expect("channel closed unexpectedly");
        drop(tx); // Close the channel
    }

    serve(args, base_engine, rx, interrupt).await
}
