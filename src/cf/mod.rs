//! Cloudflare Workers entrypoint for http-nu (gated by `cloudflare` feature).
//!
//! This module is *additive* and never imported on desktop. It uses the same
//! `crate::Engine` desktop uses -- no clean-room copy. All http-nu custom
//! commands (`.bus pub`, `.mj`, `.md`, `.highlight`, `to sse`, etc.) come
//! along automatically via `add_custom_commands()`.
//!
//! What's still TODO: streaming response bodies, `.static` via R2 / Vfs,
//! `.bus sub` via a BusDO, request body as a Nu pipeline. See CLOUDFLARE.md.

use std::collections::HashMap;

use worker::{Context, Env, Request as WorkerRequest, Response, Result};

use crate::engine::Engine;
use crate::request::{request_to_value, Request};
use crate::response::value_to_bytes;

const HANDLER_SCRIPT: &str = r#"{|req| $"hello: ($req.method) ($req.path)"}"#;

#[worker::event(fetch)]
async fn fetch(req: WorkerRequest, _env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    match handle(&req) {
        Ok(body) => Response::ok(body),
        Err(err) => Response::error(err, 500),
    }
}

fn handle(req: &WorkerRequest) -> std::result::Result<String, String> {
    let mut engine = Engine::new().map_err(|e| format!("engine: {e}"))?;
    engine
        .add_custom_commands()
        .map_err(|e| format!("commands: {e}"))?;
    engine
        .parse_closure(HANDLER_SCRIPT, None)
        .map_err(|e| format!("parse: {e}"))?;

    let req_struct = worker_request_to_http_nu(req)?;
    let req_value = request_to_value(&req_struct, nu_protocol::Span::unknown());

    let pd = engine
        .run_closure(req_value, nu_protocol::PipelineData::Empty)
        .map_err(|e| format!("eval: {e}"))?;

    let value = pd
        .into_value(nu_protocol::Span::unknown())
        .map_err(|e| format!("into_value: {e}"))?;

    let bytes = value_to_bytes(value);
    String::from_utf8(bytes).map_err(|e| format!("utf8: {e}"))
}

fn worker_request_to_http_nu(req: &WorkerRequest) -> std::result::Result<Request, String> {
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
