use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Handler {
    engine: Arc<crate::Engine>,
    response: Arc<Mutex<Option<crate::Response>>>,
}

impl Handler {
    pub fn new(engine: crate::Engine) -> Self {
        Self {
            engine: Arc::new(engine),
            response: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn handle_request(
        &self,
        req: hyper::Request<hyper::Body>,
    ) -> Result<hyper::Response<hyper::Body>, crate::Error> {
        let (parts, body) = req.into_parts();

        let request = crate::Request {
            method: parts.method.to_string(),
            uri: parts.uri.to_string(),
            path: parts.uri.path().to_string(),
            headers: parts
                .headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
                .collect(),
            query: parts
                .uri
                .query()
                .map(|q| {
                    url::form_urlencoded::parse(q.as_bytes())
                        .into_owned()
                        .collect()
                })
                .unwrap_or_default(),
        };

        // Create response building infrastructure
        let response = self.response.clone();

        // Run in separate thread
        let engine = self.engine.clone();
        let result = tokio::task::spawn_blocking(move || {
            engine.eval_closure("{ echo 'hello world' }".into(), request)
        })
        .await??;

        let response = response.lock().await;
        let status = response.as_ref().map(|r| r.status).unwrap_or(200);

        Ok(hyper::Response::builder()
            .status(status)
            .body(hyper::Body::from(format!("{:?}", result)))?)
    }
}
