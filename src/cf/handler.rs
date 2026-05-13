//! Request handler for the wasm/CF target. Mirrors `src/handler.rs`:
//!   - intercepts the Datastar JS bundle path before Nu eval
//!   - otherwise: builds a Nu `$req` record, runs the cached closure,
//!     turns the resulting `PipelineData` into a `worker::Response`.
//!
//! Engine caching + the `#[event(fetch)]` entry live in `src/cf/mod.rs`
//! (mirrors the role `src/main.rs` plays for desktop).

use worker::{Method, Request as WorkerRequest, Response, Result};

use crate::commands::RESPONSE_TX;
use crate::request::request_to_value;
use crate::response::{
    extract_http_response_meta, infer_content_type, HeaderValue, ResponseBodyType,
};

use super::request::worker_request_to_http_nu;
use super::response::{body_to_pipeline, build_response};
use crate::vfs::with_vfs;

// Datastar JS bundle, embedded the same way desktop does in src/handler.rs.
// Workers gzip/brotli is applied transparently by the runtime so we serve
// the uncompressed bundle here -- no Accept-Encoding negotiation in the
// worker code.
const DATASTAR_JS_SUFFIX: &str = "/datastar@1.0.1.js";
const DATASTAR_JS: &[u8] = include_bytes!("../stdlib/datastar/datastar@1.0.1.js");

// PUT <user>/admin/handler -- accepts a Nu closure as request body and
// hot-swaps it into the cached engine for THIS user's isolate. Each
// UserSpace DO has its own wasm isolate + ENGINE OnceLock; a swap stays
// scoped to that user. New warm isolates restart from the compiled-in
// HANDLER_SCRIPT. Useful for uploading a per-user handler:
//   curl -X PUT --data-binary @serve.nu https://.../<user>/admin/handler
const ADMIN_HANDLER_SUFFIX: &str = "/admin/handler";

/// Strip the leading `/<user>` from a path so we can match route
/// suffixes uniformly regardless of which user's DO we're in.
/// "/alice/admin/handler" -> "/admin/handler"
/// "/alice"               -> "/"
/// "/"                    -> "/"
fn route_suffix(path: &str) -> String {
    let mut parts = path.splitn(3, '/');
    parts.next(); // leading empty
    parts.next(); // user_id
    match parts.next() {
        Some(rest) if !rest.is_empty() => format!("/{rest}"),
        _ => "/".to_string(),
    }
}

/// Top-level request handler. Errors here become 500 in the fetch event.
pub(super) async fn handle(req: &mut WorkerRequest) -> Result<Response> {
    let path = req.url().map(|u| u.path().to_string()).unwrap_or_default();
    let suffix = route_suffix(&path);

    // Short-circuit the Datastar JS bundle (matches /<user>/datastar@1.0.1.js).
    if suffix == DATASTAR_JS_SUFFIX {
        return datastar_js_response();
    }

    // Read body upfront for non-GET-shape methods so we can drive the Nu
    // pipeline synchronously below.
    let body = match req.method() {
        Method::Get | Method::Head | Method::Options => Vec::new(),
        _ => req.bytes().await.unwrap_or_default(),
    };

    if suffix == ADMIN_HANDLER_SUFFIX && matches!(req.method(), Method::Put | Method::Post) {
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
    // Wire RESPONSE_TX so commands like `.static` (src/commands.rs) can
    // override the response with body_type::Static. We mirror desktop's
    // pattern in src/worker.rs. After eval, we check the receiver: if
    // the closure sent a Static response, we read from Workspace and
    // build a worker::Response from those bytes. Otherwise we fall
    // through to the normal PipelineData -> Response path.
    let (resp_tx, mut resp_rx) = tokio::sync::oneshot::channel::<crate::response::Response>();
    RESPONSE_TX.with(|cell| *cell.borrow_mut() = Some(resp_tx));

    // Run the closure under the engine lock, then drop it before we
    // build the response. Streams returned by run_closure carry their
    // own 'static iterator, so they're valid after the lock is released.
    let pd_result = {
        let engine = super::engine()
            .lock()
            .map_err(|_| "engine mutex poisoned".to_string())?;
        let req_struct = worker_request_to_http_nu(req)?;
        let req_value = request_to_value(&req_struct, nu_protocol::Span::unknown());
        let pipeline_input = body_to_pipeline(body, &engine);
        engine.run_closure(req_value, pipeline_input)
    };

    // Always clear our slot so a leftover sender doesn't leak across
    // requests on the same isolate.
    RESPONSE_TX.with(|cell| {
        let _ = cell.borrow_mut().take();
    });

    // If the closure used `.static` (or another command that emitted a
    // Response via RESPONSE_TX), serve that. We check first because the
    // closure's PipelineData return value is discarded in that case
    // (matching desktop semantics).
    if let Ok(early) = resp_rx.try_recv() {
        return build_early_response(early);
    }

    // Normal PipelineData -> worker::Response path.
    let pd = pd_result.map_err(|e| format!("eval: {e}"))?;
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

/// Build a worker::Response from a crate::response::Response that a Nu
/// command sent via RESPONSE_TX. Today only body_type::Static is handled
/// (served from Workspace via SnapshotVfs); ReverseProxy could land
/// here too in the future.
fn build_early_response(early: crate::response::Response) -> std::result::Result<Response, String> {
    let body_bytes = match &early.body_type {
        ResponseBodyType::Static {
            root,
            path,
            fallback,
        } => serve_static_from_snapshot(root, path, fallback.as_deref())?,
        ResponseBodyType::Normal => Vec::new(),
        ResponseBodyType::ReverseProxy { .. } => {
            return Err("reverse proxy not supported on CF target".into());
        }
    };

    // Infer Content-Type from the path (extension), then let explicit
    // headers from the command override.
    let inferred_ct = match &early.body_type {
        ResponseBodyType::Static { path, .. } => Some(content_type_for(path)),
        _ => None,
    };

    let mut response = Response::from_bytes(body_bytes).map_err(|e| format!("from_bytes: {e}"))?;
    response = response.with_status(early.status);
    let headers = response.headers_mut();
    if let Some(ct) = inferred_ct {
        let _ = headers.set("Content-Type", &ct);
    }
    for (k, v) in &early.headers {
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

fn serve_static_from_snapshot(
    root: &std::path::Path,
    request_path: &str,
    fallback: Option<&str>,
) -> std::result::Result<Vec<u8>, String> {
    let root_str = root.to_string_lossy();
    let root_norm = if root_str.starts_with('/') {
        root_str.into_owned()
    } else {
        format!("/{root_str}")
    };
    let path_norm = if request_path.starts_with('/') {
        request_path.to_string()
    } else {
        format!("/{request_path}")
    };
    let full = if root_norm == "/" {
        path_norm.clone()
    } else if path_norm == "/" {
        root_norm.clone()
    } else {
        format!("{}{}", root_norm.trim_end_matches('/'), path_norm)
    };

    let primary = with_vfs(|v| v.and_then(|v| v.read_bytes(std::path::Path::new(&full)).ok()));
    if let Some(b) = primary {
        return Ok(b);
    }
    // Directory-style request: GET /foo/ or extension-less path. Try
    // `<full>/index.html` (typical static-site server behaviour).
    let looks_like_dir = full.ends_with('/')
        || !std::path::Path::new(&full)
            .file_name()
            .map(|n| n.to_string_lossy().contains('.'))
            .unwrap_or(false);
    if looks_like_dir {
        let index = if full.ends_with('/') {
            format!("{full}index.html")
        } else {
            format!("{full}/index.html")
        };
        if let Some(b) =
            with_vfs(|v| v.and_then(|v| v.read_bytes(std::path::Path::new(&index)).ok()))
        {
            return Ok(b);
        }
    }
    if let Some(fb) = fallback {
        let fb_path = if fb.starts_with('/') {
            fb.to_string()
        } else {
            format!("{root_norm}/{fb}")
        };
        if let Some(b) =
            with_vfs(|v| v.and_then(|v| v.read_bytes(std::path::Path::new(&fb_path)).ok()))
        {
            return Ok(b);
        }
    }
    Err(format!(".static: {full} not found in workspace"))
}

fn content_type_for(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
    .to_string()
}
