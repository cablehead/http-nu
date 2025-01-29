use http_body_util::{combinators::BoxBody, BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Request, Response};

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type HTTPResult = Result<Response<BoxBody<Bytes, BoxError>>, BoxError>;

pub async fn handle<B>(engine: crate::Engine, req: Request<B>) -> HTTPResult
where
    B: hyper::body::Body + Send + 'static,
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

    let result = tokio::task::spawn_blocking(move || engine.eval(request)).await??;

    Ok(hyper::Response::builder().status(200).body(
        Full::new(format!("{:?}", result).into())
            .map_err(|never| match never {})
            .boxed(),
    )?)
}
