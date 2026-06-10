// Throughput benchmarks for the request hot path.
//
// Run with: cargo bench --bench throughput
//
// Dimensions, each over N requests against an in-process `handle()` with a
// hello-world closure (no TCP, no TLS -- this isolates the engine/handler
// path: request -> eval thread -> closure eval -> response):
//
//   hello            sequential requests; per-request latency of the eval path
//   hello-concurrent 16 requests in flight; what the path sustains across cores
//
// Output is one parseable line per dimension:
//   <name> requests=<n> ms=<elapsed> req_per_s=<rate> us_per_req=<cost>
//
// Numbers are only comparable on the same hardware.

use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper::Request;

use http_nu::commands::{MjCommand, PrintCommand, StaticCommand, ToSse};
use http_nu::handler::{handle, AppConfig};
use http_nu::Engine;

const N: usize = 10_000;

fn report(name: &str, n: usize, elapsed: Duration) {
    let ms = elapsed.as_secs_f64() * 1e3;
    let rate = n as f64 / elapsed.as_secs_f64();
    let us = elapsed.as_secs_f64() * 1e6 / n as f64;
    println!("{name} requests={n} ms={ms:.0} req_per_s={rate:.0} us_per_req={us:.2}");
}

fn hello_engine() -> Engine {
    let mut engine = Engine::new().unwrap();
    engine
        .add_commands(vec![
            Box::new(StaticCommand::new()),
            Box::new(ToSse {}),
            Box::new(MjCommand::new()),
            Box::new(PrintCommand::new()),
        ])
        .unwrap();
    engine
        .set_http_nu_const(&http_nu::engine::HttpNuOptions::default())
        .unwrap();
    engine
        .parse_closure(r#"{|req| "hello world" }"#, None)
        .unwrap();
    engine
}

fn config() -> Arc<AppConfig> {
    Arc::new(AppConfig {
        trusted_proxies: vec![],
        datastar: false,
        dev: false,
    })
}

async fn one_request(engine: Arc<ArcSwap<Engine>>, config: Arc<AppConfig>) {
    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();
    let resp = handle(engine, None, config, req).await.unwrap();
    assert_eq!(resp.status(), 200);
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], b"hello world");
}

async fn bench_hello(engine: Arc<ArcSwap<Engine>>, config: Arc<AppConfig>) {
    let start = Instant::now();
    for _ in 0..N {
        one_request(engine.clone(), config.clone()).await;
    }
    report("hello", N, start.elapsed());
}

async fn bench_hello_concurrent(engine: Arc<ArcSwap<Engine>>, config: Arc<AppConfig>) {
    const IN_FLIGHT: usize = 16;
    let start = Instant::now();
    let mut handles = Vec::with_capacity(IN_FLIGHT);
    for _ in 0..IN_FLIGHT {
        let engine = engine.clone();
        let config = config.clone();
        handles.push(tokio::spawn(async move {
            for _ in 0..N / IN_FLIGHT {
                one_request(engine.clone(), config.clone()).await;
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
    report(
        "hello-concurrent",
        N / IN_FLIGHT * IN_FLIGHT,
        start.elapsed(),
    );
}

fn main() {
    let engine = Arc::new(ArcSwap::from_pointee(hello_engine()));
    let config = config();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        // warm up: first eval pays one-time parse/compile costs
        one_request(engine.clone(), config.clone()).await;
        bench_hello(engine.clone(), config.clone()).await;
        bench_hello_concurrent(engine, config).await;
    });
}
