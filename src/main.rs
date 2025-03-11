use std::io::Read;
use std::path::PathBuf;

use hyper::service::service_fn;
use hyper_util::rt::TokioIo;

use clap::Parser;

use http_nu::{
    handler::{handle, ResponseStartCommand, StaticCommand},
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

async fn serve(args: Args, engine: Engine) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    loop {
        match listener.accept().await {
            Ok((stream, remote_addr)) => {
                let io = TokioIo::new(stream);
                let engine = engine.clone();

                tokio::task::spawn(async move {
                    let service = service_fn(move |req| handle(engine.clone(), remote_addr, req));
                    if let Err(err) = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, service)
                        .await
                    {
                        eprintln!("Connection error: {}", err);
                    }
                });
            }
            Err(e) => {
                eprintln!("Error accepting connection: {}", e);
                continue;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    ])?;
    engine.parse_closure(&closure_content)?;

    serve(args, engine).await
}
