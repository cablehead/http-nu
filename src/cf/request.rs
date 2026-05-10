//! Adapter: `worker::Request` -> http-nu's `crate::request::Request`.
//! Body is *not* read here -- the fetch handler reads it eagerly so the
//! sync Nu pipeline can consume it. See `super::body_to_pipeline`.

use std::collections::HashMap;

use worker::Request as WorkerRequest;

use crate::request::Request;

pub(super) fn worker_request_to_http_nu(
    req: &WorkerRequest,
) -> std::result::Result<Request, String> {
    let url = req.url().map_err(|e| format!("url: {e}"))?;
    let method_str = req.method().to_string();
    let method = http::method::Method::from_bytes(method_str.as_bytes())
        .map_err(|e| format!("method: {e}"))?;

    let mut headers = http::header::HeaderMap::new();
    for (k, v) in req.headers() {
        if let (Ok(name), Ok(value)) = (
            http::header::HeaderName::from_bytes(k.as_bytes()),
            http::header::HeaderValue::from_str(&v),
        ) {
            headers.insert(name, value);
        }
    }

    let uri: http::Uri = url
        .as_str()
        .parse()
        .map_err(|e: http::uri::InvalidUri| format!("uri: {e}"))?;

    let mut query = HashMap::new();
    for (k, v) in url.query_pairs() {
        query.insert(k.into_owned(), v.into_owned());
    }

    Ok(Request {
        proto: "HTTP/1.1".to_string(),
        method,
        authority: url.host_str().map(|h| h.to_string()),
        remote_ip: None,
        remote_port: None,
        trusted_ip: None,
        headers,
        uri,
        path: url.path().to_string(),
        query,
    })
}
