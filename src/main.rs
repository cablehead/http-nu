use std::io::Seek;
use std::path::PathBuf;
use std::sync::Arc;

use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio_rustls::TlsAcceptor;

use clap::Parser;

use http_nu::{
    handler::{handle, ResponseStartCommand},
    listener::AsyncReadWrite,
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

    /// Nushell closure to handle requests
    #[clap(value_parser)]
    closure: String,
}

fn configure_tls(pem: PathBuf) -> Result<TlsAcceptor, Box<dyn std::error::Error + Send + Sync>> {
    let pem = std::fs::File::open(pem)?;
    let mut pem = std::io::BufReader::new(pem);

    // Read certificates
    let mut certs = Vec::new();
    for cert in rustls_pemfile::certs(&mut pem) {
        certs.push(cert?);
    }

    // Reset reader to start
    pem.seek(std::io::SeekFrom::Start(0))?;

    // Read private key
    let key = rustls_pemfile::private_key(&mut pem)?.ok_or("No private key found in PEM file")?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(TlsAcceptor::from(Arc::new(config)))
}

async fn serve(args: Args, engine: Engine) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut listener = Listener::bind(&args.addr).await?;
    println!("Listening on {}", listener);

    // Configure TLS if enabled
    let tls_acceptor = if let Some(pem_path) = args.tls {
        Some(configure_tls(pem_path)?)
    } else {
        None
    };

    while let Ok((stream, remote_addr)) = listener.accept().await {
        let stream = if let Some(tls) = &tls_acceptor {
            // Handle TLS connection
            match tls.accept(stream).await {
                Ok(tls_stream) => Box::new(tls_stream) as Box<dyn AsyncReadWrite + Send + Unpin>,
                Err(e) => {
                    eprintln!("TLS error: {}", e);
                    continue;
                }
            }
        } else {
            // Handle plain TCP connection
            stream
        };

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

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    let mut engine = Engine::new()?;
    engine.add_commands(vec![Box::new(ResponseStartCommand::new())])?;
    engine.parse_closure(&args.closure)?;

    serve(args, engine).await
}
