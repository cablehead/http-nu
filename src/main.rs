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

    let mut engine = Engine::new()?;
    engine.parse_closure(&args.closure)?;

    let mut listener = Listener::bind(&args.addr).await?;
    println!("Listening on {}", listener);

    while let Ok((stream, _)) = listener.accept().await {
        let io = hyper_util::rt::TokioIo::new(stream);

        let engine = engine.clone();

        tokio::task::spawn(async move {
            let service = service_fn(move |req| handle(engine.clone(), req));
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
