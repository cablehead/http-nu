use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Bytes;
use std::sync::Arc;
use tokio::sync::Mutex;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

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
        req: hyper::Request<hyper::body::Incoming>,
    ) -> Result<hyper::Response<BoxBody<Bytes, BoxError>>, BoxError> {
        let (parts, _body) = req.into_parts();

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

        let response = self.response.clone();
        let engine = self.engine.clone();
        let result = tokio::task::spawn_blocking(move || {
            engine.eval_closure("{ echo 'hello world' }".into(), request)
        })
        .await??;

        let response = response.lock().await;
        let status = response.as_ref().map(|r| r.status).unwrap_or(200);

        Ok(hyper::Response::builder().status(status).body(
            Full::new(format!("{:?}", result).into())
                .map_err(|never| match never {})
                .boxed(),
        )?)
    }
}
