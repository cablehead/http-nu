#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http_body_util::BodyExt;
    use hyper::Request;

    #[tokio::test]
    async fn test_handle_request() {
        let engine = crate::Engine::new().unwrap();
        let handler = crate::Handler::new(engine);

        let req = Request::builder()
            .method("GET")
            .uri("/")
            .body(hyper::body::Incoming::default())
            .unwrap();

        let resp = handler.handle_request(req).await.unwrap();
        assert_eq!(resp.status(), 200);

        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("hello world"));
    }
}
