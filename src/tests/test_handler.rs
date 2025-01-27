#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{Body, Request};

    #[tokio::test]
    async fn test_handle_request() {
        let engine = crate::Engine::new().unwrap();
        let handler = crate::Handler::new(engine);

        let req = Request::builder()
            .method("GET")
            .uri("/")
            .body(Body::empty())
            .unwrap();

        let resp = handler.handle_request(req).await.unwrap();

        assert_eq!(resp.status(), 200);

        let body = hyper::body::to_bytes(resp.into_body()).await.unwrap();
        let body = String::from_utf8(body.to_vec()).unwrap();
        assert!(body.contains("hello world"));
    }
}
