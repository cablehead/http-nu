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
        // Strip the leading `/<user_id>` segment so Nu closures see
        // paths mounted at root (`/hello`) the same way they do on
        // desktop, regardless of which user's DO they landed in.
        // Closures never see the user prefix; debug routes
        // (`/<user>/_workspace/*`) and the admin handler upload are
        // handled by `cf::mod.rs::fetch` BEFORE the closure runs,
        // so this strip is only applied to real handler invocations.
        path: strip_user_prefix(url.path()),
        query,
    })
}

/// "/alice/foo/bar" -> "/foo/bar"
/// "/alice/"        -> "/"
/// "/alice"         -> "/"
/// "/"              -> "/"
fn strip_user_prefix(path: &str) -> String {
    let mut parts = path.splitn(3, '/');
    parts.next(); // leading empty
    parts.next(); // user_id
    match parts.next() {
        Some(rest) if !rest.is_empty() => format!("/{rest}"),
        _ => "/".to_string(),
    }
}
