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

    let resp = handle(engine, r#"{|request| "hello world" }"#.into(), None, req)
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
    let script = r#"{|request|
        match $request {
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

    // Test unmatched route should 404
    let req = Request::builder()
        .method("GET")
        .uri("/foo")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handle(engine, script, None, req).await.unwrap();

    // Verify 404 response
    assert_eq!(resp.status(), 404);

    // Verify empty body
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(body.len(), 0);
}
