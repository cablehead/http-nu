//! Request handler for the wasm/CF target. Mirrors `src/handler.rs`:
//!   - intercepts the Datastar JS bundle path before Nu eval
//!   - otherwise: builds a Nu `$req` record, runs the cached closure,
//!     turns the resulting `PipelineData` into a `worker::Response`.
//!
//! Engine caching + the `#[event(fetch)]` entry live in `src/cf/mod.rs`
//! (mirrors the role `src/main.rs` plays for desktop).

use worker::{Method, Request as WorkerRequest, Response, Result};

use crate::request::request_to_value;
use crate::response::{extract_http_response_meta, infer_content_type, HeaderValue};

use super::request::worker_request_to_http_nu;
use super::response::{body_to_pipeline, build_response};

// Datastar JS bundle, embedded the same way desktop does in src/handler.rs.
// Workers gzip/brotli is applied transparently by the runtime so we serve
// the uncompressed bundle here -- no Accept-Encoding negotiation in the
// worker code.
const DATASTAR_JS_PATH: &str = "/datastar@1.0.1.js";
const DATASTAR_JS: &[u8] = include_bytes!("../stdlib/datastar/datastar@1.0.1.js");

// PUT /admin/handler -- accepts a Nu closure as request body and hot-swaps
// it into the cached engine for this isolate. Per-isolate only: new warm
// isolates restart from the compiled-in HANDLER_SCRIPT. Useful for live
// editing during development via `curl -X PUT ... --data-binary @script.nu`.
const ADMIN_HANDLER_PATH: &str = "/admin/handler";

/// Top-level request handler. Errors here become 500 in the fetch event.
pub(super) async fn handle(req: &mut WorkerRequest) -> Result<Response> {
    let path = req.url().map(|u| u.path().to_string()).unwrap_or_default();

    // Short-circuit the Datastar JS bundle, byte-identical to desktop's
    // route in src/handler.rs. Always served on CF (no `--datastar`
    // toggle wired yet; whoever built with `--features cloudflare`
    // gets the full http-nu surface).
    if path == DATASTAR_JS_PATH {
        return datastar_js_response();
    }

    // Read body upfront for non-GET-shape methods so we can drive the Nu
    // pipeline synchronously below.
    let body = match req.method() {
        Method::Get | Method::Head | Method::Options => Vec::new(),
        _ => req.bytes().await.unwrap_or_default(),
    };

    if path == ADMIN_HANDLER_PATH && matches!(req.method(), Method::Put | Method::Post) {
        let script = match String::from_utf8(body) {
            Ok(s) => s,
            Err(_) => return Response::error("body must be valid UTF-8", 400),
        };
        return match swap_handler(&script) {
            Ok(r) => Ok(r),
            Err(e) => Response::error(e, 400),
        };
    }

    match run_closure(req, body) {
        Ok(response) => Ok(response),
        Err(err) => Response::error(err, 500),
    }
}

fn swap_handler(script: &str) -> std::result::Result<Response, String> {
    let mut engine = super::engine()
        .lock()
        .map_err(|_| "engine mutex poisoned".to_string())?;
    engine
        .parse_closure(script, None)
        .map_err(|e| format!("parse error: {e}"))?;
    Response::ok("ok").map_err(|e| e.to_string())
}

fn datastar_js_response() -> Result<Response> {
    let mut response = Response::from_bytes(DATASTAR_JS.to_vec())?;
    let headers = response.headers_mut();
    let _ = headers.set("Content-Type", "application/javascript");
    let _ = headers.set("Cache-Control", "public, max-age=31536000, immutable");
    Ok(response)
}

fn run_closure(req: &WorkerRequest, body: Vec<u8>) -> std::result::Result<Response, String> {
    // Run the closure under the engine lock, then drop it before we
    // build the response. Streams returned by run_closure carry their
    // own 'static iterator, so they're valid after the lock is released.
    let pd = {
        let engine = super::engine()
            .lock()
            .map_err(|_| "engine mutex poisoned".to_string())?;
        let req_struct = worker_request_to_http_nu(req)?;
        let req_value = request_to_value(&req_struct, nu_protocol::Span::unknown());
        let pipeline_input = body_to_pipeline(body, &engine);
        engine
            .run_closure(req_value, pipeline_input)
            .map_err(|e| format!("eval: {e}"))?
    };

    let http_meta = extract_http_response_meta(pd.metadata_ref());
    let inferred_ct = infer_content_type(&pd);

    let (response, jsonl_ct_override) = build_response(pd)?;
    let mut response = response;
    if let Some(status) = http_meta.status {
        response = response.with_status(status);
    }
    let headers = response.headers_mut();
    if let Some(ct) = jsonl_ct_override.or(inferred_ct) {
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
