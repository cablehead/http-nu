use std::io::Read;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use axum::{routing::any, Router};
use clap::Parser;
use http_nu::{
    commands::{ResponseStartCommand, StaticCommand, ToSse},
    handler::handle,
    listener::TlsConfig,
    Engine, Listener,
};
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::signal;
use tower::Service;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Set up Ctrl-C handling for clean shutdown
    let interrupt = setup_ctrlc_handler(&mut engine)?;

    let engine = Arc::new(engine);
    let app = Router::new().fallback(any(move |req| handle(engine, None, req)));

    // Configure TLS if enabled
    let tls_config = if let Some(pem_path) = args.tls {
        Some(TlsConfig::from_pem(pem_path)?)
    } else {
        None
    };

    let listener = Listener::bind(&args.addr, tls_config).await?;
    println!(
        "{}",
        serde_json::json!({"stamp": scru128::new(), "message": "start", "address": format!("{}", listener)})
    );

    use axum_server::tls_rustls::RustlsConfig;

    // ...

    match listener {
        Listener::Tcp {
            listener,
            tls_config,
        } => {
            let listener = Arc::try_unwrap(listener).expect("listener is not shared");
            if let Some(tls) = tls_config {
                let config = RustlsConfig::from_config(tls.config);
                axum_server::from_tcp_rustls(listener.into_std()?, config)
                    .serve(app.into_make_service())
                    .await?;
            } else {
                axum::serve(listener, app.into_make_service())
                    .with_graceful_shutdown(shutdown_signal(interrupt.clone()))
                    .await?;
            }
        }
        #[cfg(unix)]
        Listener::Unix(listener) => {
            let shutdown = shutdown_signal(interrupt.clone());
            tokio::pin!(shutdown);

            let service = service_fn(move |req| {
                let mut app = app.clone();
                async move { app.call(req).await }
            });

            loop {
                tokio::select! {
                    res = listener.accept() => {
                        match res {
                            Ok((stream, _addr)) => {
                                let io = TokioIo::new(stream);
                                let service = service.clone();

                                tokio::task::spawn(async move {
                                    if let Err(err) = hyper::server::conn::http1::Builder::new()
                                        .serve_connection(io, service)
                                        .await
                                    {
                                        eprintln!("Connection error: {err}");
                                    }
                                });
                            }
                            Err(e) => {
                                eprintln!("Error accepting connection: {e}");
                                continue;
                            }
                        }
                    },
                    _ = &mut shutdown => {
                        break;
                    }
                }
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
    let args = Args::parse();

    // Determine the closure source
    let closure_content = if args.closure == "-" {
        // Read closure from stdin
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        buffer
    } else {
        // Use the closure provided as argument
        args.closure.clone()
    };

    let mut engine = Engine::new()?;
    engine.add_commands(vec![
        Box::new(ResponseStartCommand::new()),
        Box::new(StaticCommand::new()),
        Box::new(ToSse {}),
    ])?;
    engine.parse_closure(&closure_content)?;

    serve(args, engine).await
}
