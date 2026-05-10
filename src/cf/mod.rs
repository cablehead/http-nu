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
//! Layout (all inside src/cf/, all CF-only -- no upstream conflict):
//!   mod.rs       this file: fetch handler + engine cache
//!   request.rs   worker::Request -> crate::request::Request adapter
//!   response.rs  PipelineData -> worker::Response (incl. streaming)
//!   wrangler.toml  Workers config
//!
//! Shared logic with desktop lives upstream, in `src/response.rs`
//! (`infer_content_type`, `value_to_bytes`, `extract_http_response_meta`).
//! When this module needs a helper that desktop also uses, the helper
//! goes upstream so both targets call the same function.
//!
//! The handler script is embedded at compile time via
//! `include_str!(env!("CF_HANDLER_PATH"))`. The engine is cached in a
//! module-level `OnceLock<Mutex<Engine>>` purely as a per-warm-isolate
//! perf optimisation -- desktop already amortises engine setup once per
//! process; this gives wasm the same property.
//!
//! Hot reload is intentionally NOT exposed through a CF-only HTTP route
//! today. Desktop reloads via `--watch` (filesystem) or `--topic` (xs
//! append), neither of which is a user-facing HTTP surface. When the
//! desktop-parity triggers land on CF (xs CF integration, or
//! `@cloudflare/shell` Workspace as the `--watch` substrate), the
//! reload mechanism plugs into the same `Mutex<Engine>` here and stays
//! invisible to the closure-author.

mod request;
mod response;

use std::sync::{Mutex, OnceLock};

use worker::{Context, Env, Method, Request as WorkerRequest, Response, Result};

use crate::engine::Engine;
use crate::request::request_to_value;
use crate::response::{extract_http_response_meta, infer_content_type, HeaderValue};

use self::request::worker_request_to_http_nu;
use self::response::{body_to_pipeline, build_response};

const HANDLER_SCRIPT: &str = include_str!(env!("CF_HANDLER_PATH"));

static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();

/// Lazily build the cached engine + parse the embedded handler. Panics
/// on first-use if engine init or handler parsing fails;
/// console_error_panic_hook surfaces a readable error to wrangler logs.
fn engine() -> &'static Mutex<Engine> {
    ENGINE.get_or_init(|| {
        let mut engine = Engine::new().expect("Engine::new failed");
        engine
            .add_custom_commands()
            .expect("add_custom_commands failed");
        engine
            .parse_closure(HANDLER_SCRIPT, None)
            .expect("handler failed to parse");
        Mutex::new(engine)
    })
}

#[worker::event(fetch)]
async fn fetch(mut req: WorkerRequest, _env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

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

fn handle(req: &WorkerRequest, body: Vec<u8>) -> std::result::Result<Response, String> {
    // Run the closure under the engine lock, then drop the lock before we
    // build the response. Streams returned by run_closure carry their own
    // 'static iterator, so they're valid after the lock is released.
    let pd = {
        let engine = engine()
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
