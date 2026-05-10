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
//!   mod.rs        this file: #[event(fetch)] entry + engine cache
//!                 (sibling of desktop's src/main.rs)
//!   handler.rs    request lifecycle, datastar JS route
//!                 (sibling of desktop's src/handler.rs)
//!   request.rs    worker::Request -> crate::request::Request adapter
//!                 (sibling of desktop's src/request.rs)
//!   response.rs   PipelineData -> worker::Response (incl. streaming)
//!                 (sibling of desktop's src/response.rs)
//!   wrangler.toml Workers config
//!
//! Shared logic with desktop lives upstream in `src/response.rs`
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

mod handler;
mod request;
mod response;

use std::sync::{Mutex, OnceLock};

use worker::{Context, Env, Request as WorkerRequest, Response, Result};

use crate::engine::Engine;

const HANDLER_SCRIPT: &str = include_str!(env!("CF_HANDLER_PATH"));

static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();

/// Lazily build the cached engine + parse the embedded handler. Panics
/// on first-use if engine init or handler parsing fails;
/// console_error_panic_hook surfaces a readable error to wrangler logs.
pub(super) fn engine() -> &'static Mutex<Engine> {
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
    handler::handle(&mut req).await
}
