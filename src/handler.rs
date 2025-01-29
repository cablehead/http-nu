use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Body;
use hyper::body::Bytes;
use std::sync::Arc;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub struct Handler {
    engine: Arc<crate::Engine>,
}

impl Handler {
    pub fn new(engine: crate::Engine) -> Self {
        Self {
            engine: Arc::new(engine),
        }
    }

    pub async fn handle_request<B>(
        &self,
        req: hyper::Request<B>,
    ) -> Result<hyper::Response<BoxBody<Bytes, BoxError>>, BoxError>
    where
        B: Body + Send + 'static,
        B::Data: Send,
        B::Error: Into<BoxError>,
    {
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

        let engine = self.engine.clone();
        let result = tokio::task::spawn_blocking(move || engine.eval(request)).await??;

        Ok(hyper::Response::builder().status(200).body(
            Full::new(format!("{:?}", result).into())
                .map_err(|never| match never {})
                .boxed(),
        )?)
    }
}
