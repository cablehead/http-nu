use clap::Parser;

use hyper::service::service_fn;

use http_nu::{handle, Engine, Listener};

#[derive(Parser, Debug)]
#[clap(version)]
struct Args {
    /// Address to listen on [HOST]:PORT or <PATH> for Unix domain socket
    #[clap(value_parser)]
    addr: String,

    /// Nushell closure to handle requests
    #[clap(value_parser)]
    closure: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = Args::parse();

    let engine = Engine::new()?;

    let mut listener = Listener::bind(&args.addr).await?;
    println!("Listening on {}", listener);

    while let Ok((stream, remote_addr)) = listener.accept().await {
        let io = hyper_util::rt::TokioIo::new(stream);

        let engine = engine.clone();
        let closure = args.closure.clone();

        tokio::task::spawn(async move {
            let service =
                service_fn(move |req| handle(engine.clone(), closure.clone(), remote_addr, req));
            if let Err(err) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, service)
                .await
            {
                eprintln!("Error serving connection: {}", err);
            }
        });
    }

    Ok(())
}
