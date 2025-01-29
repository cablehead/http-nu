use clap::Parser;
use http_nu::{Engine, Handler};

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
    let handler = Handler::new(engine);

    let mut listener = http_nu::listener::Listener::bind(&args.addr).await?;
    println!("Listening on {}", listener);

    while let Ok((stream, _)) = listener.accept().await {
        let io = hyper_util::rt::TokioIo::new(stream);
        let handler = handler.clone();

        tokio::task::spawn(async move {
            if let Err(err) = hyper::server::conn::http1::Builder::new()
                .serve_connection(io, handler)
                .await
            {
                eprintln!("Error serving connection: {}", err);
            }
        });
    }

    Ok(())
}
