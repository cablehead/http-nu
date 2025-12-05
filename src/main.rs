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
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::signal;
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

/// Sets up Ctrl-C handling for clean shutdown
fn setup_ctrlc_handler(
    engine: &mut Engine,
) -> Result<Arc<AtomicBool>, Box<dyn std::error::Error + Send + Sync>> {
    let interrupt = Arc::new(AtomicBool::new(false));
    engine.set_signals(interrupt.clone());

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

    Ok(interrupt)
}

async fn serve(
    args: Args,
    mut engine: Engine,
    read_stdin: bool,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Set up Ctrl-C handling for clean shutdown
    let interrupt = setup_ctrlc_handler(&mut engine)?;

    let engine = Arc::new(ArcSwap::from_pointee(engine));

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

    // Spawn stdin reading task if enabled
    if read_stdin {
        let engine_clone = engine.clone();
        let interrupt_clone = interrupt.clone();
        tokio::spawn(async move {
            let stdin = tokio::io::stdin();
            let mut reader = BufReader::new(stdin);
            let mut buffer = Vec::new();

            loop {
                buffer.clear();
                match reader.read_until(b'\0', &mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(_n) => {
                        // Remove null terminator if present
                        if buffer.last() == Some(&b'\0') {
                            buffer.pop();
                        }

                        let script = String::from_utf8_lossy(&buffer);

                        // Create new engine with updated script
                        match Engine::new() {
                            Ok(mut new_engine) => {
                                // Connect interrupt signal to new engine
                                new_engine.set_signals(interrupt_clone.clone());

                                // Add commands
                                if let Err(e) = new_engine.add_commands(vec![
                                    Box::new(ResponseStartCommand::new()),
                                    Box::new(ReverseProxyCommand::new()),
                                    Box::new(StaticCommand::new()),
                                    Box::new(ToSse {}),
                                ]) {
                                    let err_str = e.to_string();
                                    eprintln!("Failed to add commands: {err_str}");
                                    println!(
                                        "{}",
                                        serde_json::json!({
                                            "stamp": scru128::new(),
                                            "message": "script_update",
                                            "status": "error",
                                            "error": nu_utils::strip_ansi_string_likely(err_str.clone())
                                        })
                                    );
                                    continue;
                                }

                                // Parse new closure
                                match new_engine.parse_closure(&script) {
                                    Ok(()) => {
                                        // Atomically swap in the new engine
                                        engine_clone.store(Arc::new(new_engine));
                                        println!(
                                            "{}",
                                            serde_json::json!({
                                                "stamp": scru128::new(),
                                                "message": "script_update",
                                                "status": "success"
                                            })
                                        );
                                    }
                                    Err(e) => {
                                        let err_str = e.to_string();
                                        eprintln!("Script update failed: {err_str}");
                                        println!(
                                            "{}",
                                            serde_json::json!({
                                                "stamp": scru128::new(),
                                                "message": "script_update",
                                                "status": "error",
                                                "error": nu_utils::strip_ansi_string_likely(err_str.clone())
                                            })
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                let err_str = e.to_string();
                                eprintln!("Failed to create new engine: {err_str}");
                                println!(
                                    "{}",
                                    serde_json::json!({
                                        "stamp": scru128::new(),
                                        "message": "script_update",
                                        "status": "error",
                                        "error": nu_utils::strip_ansi_string_likely(err_str.clone())
                                    })
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading stdin: {e}");
                        println!(
                            "{}",
                            serde_json::json!({
                                "stamp": scru128::new(),
                                "message": "stdin_error",
                                "error": e.to_string()
                            })
                        );
                        break;
                    }
                }
            }
        });
    }

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
    // nu_command uses its own CRYPTO_PROVIDER and expects 'ring' provider
    nu_command::tls::CRYPTO_PROVIDER
        .default()
        .then_some(())
        .expect("failed to set nu_command crypto provider");

    let args = Args::parse();

    // Determine if we should read from stdin continuously
    let read_stdin = args.closure == "-";

    // Determine the initial closure source
    let closure_content = if read_stdin {
        // Read initial closure from stdin (up to first null byte)
        let mut buffer = Vec::new();
        let mut stdin = std::io::stdin();
        let mut byte = [0u8; 1];

        loop {
            match stdin.read(&mut byte) {
                Ok(0) => {
                    // EOF without null byte - use what we have
                    break;
                }
                Ok(_) => {
                    if byte[0] == b'\0' {
                        // Found null terminator
                        break;
                    }
                    buffer.push(byte[0]);
                }
                Err(e) => {
                    eprintln!("Error reading stdin: {e}");
                    std::process::exit(1);
                }
            }
        }

        String::from_utf8_lossy(&buffer).to_string()
    } else {
        // Use the closure provided as argument
        args.closure.clone()
    };

    let mut engine = Engine::new()?;
    engine.add_commands(vec![
        Box::new(ResponseStartCommand::new()),
        Box::new(ReverseProxyCommand::new()),
        Box::new(StaticCommand::new()),
        Box::new(ToSse {}),
        Box::new(MjCommand::new()),
    ])?;

    if let Err(e) = engine.parse_closure(&closure_content) {
        eprintln!("{e}");
        std::process::exit(1);
    }

    serve(args, engine, read_stdin).await
}
