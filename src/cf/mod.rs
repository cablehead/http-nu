//! Cloudflare Workers entrypoint for http-nu.
//!
//! Build / deploy:
//!   mise run cf:build       # worker-build --features cloudflare
//!   mise run cf:dev         # wrangler dev
//!   mise run cf:deploy      # wrangler deploy
//!
//! This module is gated to `cf(all(feature = "cloudflare",
//! target_arch = "wasm32"))` and is the *only* CF-specific code in the
//! tree. It is additive — never imported on desktop, never edits an
//! upstream file. It calls `crate::Engine` directly so all custom
//! commands (`.bus pub`, `.mj`, `.md`, `.highlight`, `to sse`, ...)
//! come along automatically.
//!
//! Today: each request rebuilds the engine, parses a hardcoded
//! `examples/blog/serve.nu`, runs the closure synchronously, returns
//! the value as a string. There is no body bridging, no response
//! streaming, no Vfs. See CLOUDFLARE.md "Status" for the full punchlist.

use std::collections::HashMap;

use worker::{Context, Env, Request as WorkerRequest, Response, Result};

use crate::engine::Engine;
use crate::request::{request_to_value, Request};
use crate::response::{extract_http_response_meta, value_to_bytes, HeaderValue};

// The handler script is embedded at compile time. The path is taken from
// the CF_HANDLER_PATH env var (relative to this file) so `mise run ex:cf:*`
// tasks can pick which example to run without editing source. Default is
// set by mise's cf:build task; eventually this comes from R2 or
// `@cloudflare/shell` at runtime instead of being baked in.
const HANDLER_SCRIPT: &str = include_str!(env!("CF_HANDLER_PATH"));

#[worker::event(fetch)]
async fn fetch(req: WorkerRequest, _env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    match handle(&req) {
        Ok(response) => Ok(response),
        Err(err) => Response::error(err, 500),
    }
}

fn handle(req: &WorkerRequest) -> std::result::Result<Response, String> {
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

    // Pull `http.response { status, headers }` and infer content-type
    // before consuming pd. Same shape as desktop's response handler.
    let http_meta = extract_http_response_meta(pd.metadata_ref());
    let content_type = infer_content_type(&pd);

    let value = pd
        .into_value(nu_protocol::Span::unknown())
        .map_err(|e| format!("into_value: {e}"))?;

    let bytes = value_to_bytes(value);
    let body = String::from_utf8(bytes).map_err(|e| format!("utf8: {e}"))?;

    let mut response = Response::ok(body).map_err(|e| format!("response: {e}"))?;
    if let Some(status) = http_meta.status {
        response = response.with_status(status);
    }
    let headers = response.headers_mut();
    if let Some(ct) = content_type {
        let _ = headers.set("Content-Type", &ct);
    }
    // Explicit headers via `metadata set { merge {'http.response': {headers: ...}}}`
    // override the inferred Content-Type (set last wins via `set`).
    for (k, v) in &http_meta.headers {
        match v {
            HeaderValue::Single(s) => {
                let _ = headers.set(k, s);
            }
            HeaderValue::Multiple(vs) => {
                for s in vs {
                    let _ = headers.append(k, s);
                }
            }
        }
    }
    Ok(response)
}

/// Mirrors src/worker.rs's content-type inference. Records with `__html`
/// are HTML; bare records and lists are JSON; binary is octet-stream;
/// everything else uses pipeline metadata's content-type if set.
fn infer_content_type(pd: &nu_protocol::PipelineData) -> Option<String> {
    use nu_protocol::{PipelineData, Value};
    match pd {
        PipelineData::Value(Value::Record { val, .. }, meta)
            if meta.as_ref().and_then(|m| m.content_type.clone()).is_none() =>
        {
            if val.get("__html").is_some() {
                Some("text/html; charset=utf-8".into())
            } else {
                Some("application/json".into())
            }
        }
        PipelineData::Value(Value::List { .. }, meta)
            if meta.as_ref().and_then(|m| m.content_type.clone()).is_none() =>
        {
            Some("application/json".into())
        }
        PipelineData::Value(Value::Binary { .. }, meta)
            if meta.as_ref().and_then(|m| m.content_type.clone()).is_none() =>
        {
            Some("application/octet-stream".into())
        }
        PipelineData::Value(_, meta) | PipelineData::ListStream(_, meta) => {
            meta.as_ref().and_then(|m| m.content_type.clone())
        }
        _ => None,
    }
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
