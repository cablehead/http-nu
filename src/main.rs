use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use axum::{routing::any, Router};
use clap::Parser;
use http_nu::{
    handler::{handle, ResponseStartCommand, StaticCommand},
    listener::TlsConfig,
    Engine, Listener, ToSse,
};
use tokio::signal;
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

async fn serve(args: Args, engine: Engine) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let engine = Arc::new(engine);
    let app = Router::new()
        .route("/*path", any(move |req| handle(engine, None, req)))
        .with_state(());

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

    match listener {
        Listener::Tcp { listener, .. } => {
            let listener = Arc::try_unwrap(listener).expect("listener is not shared");
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await?;
        }
        #[cfg(unix)]
        Listener::Unix(listener) => {
            let listener = Arc::try_unwrap(listener).expect("listener is not shared");
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await?;
        }
    }

    Ok(())
}

async fn shutdown_signal() {
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

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
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
