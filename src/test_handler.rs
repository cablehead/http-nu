use bytes::Bytes;

use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::Request;

use crate::handle;

#[tokio::test]
async fn test_handle() {
    let engine = crate::Engine::new().unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine, r#"{|req| "hello world" }"#.into(), None, req)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("hello world"));
}

#[tokio::test]
async fn test_handle_with_response_start() {
    let engine = crate::Engine::new().unwrap();
    let script = r#"{|req|
        match $req {
            {uri: "/resource" method: "POST"} => {
                .response {
                    status: 201
                    headers: {
                    "Content-Type": "text/plain"
                    "X-Custom": "test"
                    }
                }
                "created resource"
            }
        }
    }"#
    .to_string();

    // Test successful POST to /resource
    let req = Request::builder()
        .method("POST")
        .uri("/resource")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine.clone(), script.clone(), None, req)
        .await
        .unwrap();

    // Verify response metadata
    assert_eq!(resp.status(), 201);
    assert_eq!(resp.headers()["content-type"], "text/plain");
    assert_eq!(resp.headers()["x-custom"], "test");

    // Verify body
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        String::from_utf8(body.to_vec()).unwrap(),
        "created resource"
    );
}

#[tokio::test]
async fn test_handle_post() {
    use http_body_util::Full;
    use hyper::body::Bytes;

    let engine = crate::Engine::new().unwrap();
    let script = r#"{|req| $in }"#.to_string();

    // Create POST request with a body
    let body = "Hello from the request body!";
    let req = Request::builder()
        .method("POST")
        .uri("/echo")
        .body(Full::new(Bytes::from(body)))
        .unwrap();

    let resp = handle(engine, script, None, req).await.unwrap();

    // Verify response status
    assert_eq!(resp.status(), 200);

    // Verify body is echoed back
    let resp_body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(String::from_utf8(resp_body.to_vec()).unwrap(), body);
}

#[tokio::test]
async fn test_handle_streaming() {
    use bytes::Bytes;
    use http_body_util::Empty;
    use std::time::Instant;
    use tokio::time::Duration;

    let engine = crate::Engine::new().unwrap();
    let script = r#"{|req|
        .response {status: 200}
        1..3 | each { |n| sleep 0.1sec; $n }
    }"#
    .to_string();

    let req = Request::builder()
        .method("GET")
        .uri("/stream")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let start_time = Instant::now();
    let resp = handle(engine, script, None, req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let mut body = resp.into_body();
    let start_time = Instant::now();
    let mut collected = Vec::new();

    loop {
        match body.frame().await {
            Some(Ok(frame)) => {
                if let Some(data) = frame.data_ref() {
                    let chunk_str = String::from_utf8(data.to_vec()).unwrap();
                    let elapsed = start_time.elapsed();
                    collected.push((chunk_str.trim().to_string(), elapsed));
                }
            }
            Some(Err(e)) => panic!("Error reading frame: {}", e),
            None => break,
        }
    }

    // Should have 3 chunks
    assert_eq!(collected.len(), 3);

    assert_eq!(collected[0].0, "1");
    assert!(collected[0].1 >= Duration::from_millis(100));
    assert!(collected[0].1 <= Duration::from_millis(150));

    assert_eq!(collected[1].0, "2");
    assert!(collected[1].1 >= Duration::from_millis(200));
    assert!(collected[1].1 <= Duration::from_millis(250));

    assert_eq!(collected[2].0, "3");
    assert!(collected[2].1 >= Duration::from_millis(300));
    assert!(collected[2].1 <= Duration::from_millis(350));
}
