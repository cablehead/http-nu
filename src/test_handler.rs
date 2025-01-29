use bytes::Bytes;
use http_body_util::BodyExt;
use http_body_util::Empty;
use hyper::Request;

#[tokio::test]
async fn test_handle_request() {
    let engine = crate::Engine::new().unwrap();
    let handler = crate::Handler::new(engine);

    // Use Empty body which implements Body trait
    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Empty::<Bytes>::new())
        .unwrap();

    let resp = handler.handle_request(req).await.unwrap();
    assert_eq!(resp.status(), 200);

    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("hello world"));
}
