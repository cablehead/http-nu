//! Cloudflare Workers entrypoint for http-nu.
//!
//! Build / deploy:
//!   mise run cf:build       # worker-build --features cloudflare
//!   mise run cf:dev         # wrangler dev
//!   mise run cf:deploy      # wrangler deploy
//!
//! Gated to `cfg(all(feature = "cloudflare", target_arch = "wasm32"))`,
//! lives entirely under `src/cf/` (additive, never edits upstream files).
//! Calls `crate::Engine` directly so all custom commands (`.bus pub`,
//! `.mj`, `.md`, `.highlight`, `to sse`, ...) come along automatically.
//!
//! The default handler script is embedded at compile time via
//! `include_str!(env!("CF_HANDLER_PATH"))`. It can be replaced at runtime
//! by `PUT /admin/handler` with the new script as the request body. The
//! engine is cached in a module-level `OnceLock<Mutex<State>>` so warm
//! requests reuse it; the new script sticks for the lifetime of the
//! isolate. See CLOUDFLARE.md "Handler script lifecycle".
//!
//! ⚠ The /admin/handler endpoint is unauthenticated. Gate it behind real
//! auth (CF Access, signed token, etc.) before exposing publicly.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use worker::{Context, Env, Method, Request as WorkerRequest, Response, Result};

use crate::engine::Engine;
use crate::request::{request_to_value, Request};
use crate::response::{extract_http_response_meta, value_to_bytes, HeaderValue};

const DEFAULT_HANDLER_SCRIPT: &str = include_str!(env!("CF_HANDLER_PATH"));

struct State {
    script: String,
    engine: Engine,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

/// Lazily build the cached engine + parse the embedded default handler.
/// Panics on first-use if engine init or default-handler parsing fails;
/// console_error_panic_hook surfaces a readable error to wrangler logs.
fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| {
        let mut engine = Engine::new().expect("Engine::new failed");
        engine
            .add_custom_commands()
            .expect("add_custom_commands failed");
        engine
            .parse_closure(DEFAULT_HANDLER_SCRIPT, None)
            .expect("default handler failed to parse");
        Mutex::new(State {
            script: DEFAULT_HANDLER_SCRIPT.to_string(),
            engine,
        })
    })
}

#[worker::event(fetch)]
async fn fetch(mut req: WorkerRequest, _env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    // Admin endpoint -- view/replace the live handler script.
    let path = req.url().map(|u| u.path().to_string()).unwrap_or_default();
    if path == "/admin/handler" {
        return match req.method() {
            Method::Get => admin_get_handler(),
            Method::Put | Method::Post => {
                let body = req.bytes().await.unwrap_or_default();
                let script = String::from_utf8(body)
                    .map_err(|e| worker::Error::RustError(format!("body utf8: {e}")))?;
                admin_put_handler(&script)
            }
            _ => Response::error("method not allowed", 405),
        };
    }

    // Read body upfront for non-GET-shape methods so we can drive the Nu
    // pipeline synchronously below.
    let body = match req.method() {
        Method::Get | Method::Head | Method::Options => Vec::new(),
        _ => req.bytes().await.unwrap_or_default(),
    };

    match handle(&req, body) {
        Ok(response) => Ok(response),
        Err(err) => Response::error(err, 500),
    }
}

fn admin_get_handler() -> Result<Response> {
    let state = state().lock().expect("state mutex poisoned");
    let mut response = Response::ok(state.script.clone())?;
    let _ = response
        .headers_mut()
        .set("Content-Type", "text/plain; charset=utf-8");
    Ok(response)
}

fn admin_put_handler(script: &str) -> Result<Response> {
    let mut new_engine = match Engine::new() {
        Ok(e) => e,
        Err(e) => return Response::error(format!("engine init: {e}"), 500),
    };
    if let Err(e) = new_engine.add_custom_commands() {
        return Response::error(format!("commands: {e}"), 500);
    }
    if let Err(e) = new_engine.parse_closure(script, None) {
        // Bad script -> 400, leave existing handler in place.
        return Response::error(format!("parse: {e}"), 400);
    }

    let mut state = state().lock().expect("state mutex poisoned");
    state.script = script.to_string();
    state.engine = new_engine;
    Response::ok("handler updated\n")
}

fn handle(req: &WorkerRequest, body: Vec<u8>) -> std::result::Result<Response, String> {
    let state = state()
        .lock()
        .map_err(|_| "state mutex poisoned".to_string())?;

    let req_struct = worker_request_to_http_nu(req)?;
    let req_value = request_to_value(&req_struct, nu_protocol::Span::unknown());

    let pipeline_input = body_to_pipeline(body, &state.engine);

    let pd = state
        .engine
        .run_closure(req_value, pipeline_input)
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

/// Build the `$in` pipeline from the request body. Empty body yields
/// `PipelineData::Empty`; any bytes go through a single-shot ByteStream
/// (the desktop path uses a tokio mpsc to stream from hyper; on Workers
/// the body is already buffered by the time we get here).
fn body_to_pipeline(body: Vec<u8>, engine: &Engine) -> nu_protocol::PipelineData {
    if body.is_empty() {
        return nu_protocol::PipelineData::Empty;
    }
    let span = nu_protocol::Span::unknown();
    let signals = engine.state.signals().clone();
    let mut consumed = false;
    let stream = nu_protocol::ByteStream::from_fn(
        span,
        signals,
        nu_protocol::ByteStreamType::Unknown,
        move |buffer: &mut Vec<u8>| {
            if !consumed {
                buffer.extend_from_slice(&body);
                consumed = true;
                Ok(true)
            } else {
                Ok(false)
            }
        },
    );
    nu_protocol::PipelineData::ByteStream(stream, None)
}

/// Content-type inference -- the wasm sibling of the match in
/// src/worker.rs (which is gated to `desktop` because of std::thread).
/// Records with `__html` are HTML; bare records and lists are JSON;
/// binary is octet-stream; everything else uses pipeline metadata's
/// content-type if set. Keep in sync with worker.rs until the two are
/// merged into src/response.rs.
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
