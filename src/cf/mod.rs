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
mod nu;
mod request;
mod response;
mod snapshot_vfs;

use cloudflare_shell::{
    EntryType, MkdirOptions, OnChange, RmOptions, WorkspaceChangeEvent, WorkspaceChangeType,
};
use cloudflare_shell_workspace::Workspace;
use nu::nu_command;
use snapshot_vfs::SnapshotVfs;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use worker::{
    durable_object, wasm_bindgen, Context, DurableObject, Env, Request as WorkerRequest, Response,
    Result, State,
};

use crate::engine::Engine;

const HANDLER_SCRIPT: &str = include_str!(env!("CF_HANDLER_PATH"));

/// Path inside each user's Workspace whose contents drive the handler
/// closure. A write/update to this path fires a Workspace `onChange`
/// event; the next request re-parses it through the cached engine
/// (effectively the CF equivalent of desktop `--watch`, with the
/// Workspace as the transport rather than inotify).
const HANDLER_SCRIPT_PATH: &str = "/serve.nu";

/// Flag set by the Workspace `onChange` listener whenever
/// `HANDLER_SCRIPT_PATH` is created or updated. Checked at the top of
/// every fetch; if set, the engine re-parses from Workspace before
/// invoking the handler. Per-isolate state, same lifetime as `ENGINE`.
static HANDLER_RELOAD_PENDING: AtomicBool = AtomicBool::new(false);

// Per-isolate engine cache. Each DO instance runs in its own wasm
// isolate, so this OnceLock is effectively per-user: alice's UserSpace
// isolate caches alice's engine, bob's caches bob's. Until the handler
// becomes user-specific we still share one HANDLER_SCRIPT, but state
// the engine builds up (parsed AST, plugin signatures) is isolated.
//
// Init can fail in three places: Engine::new (rare), add_commands (rare),
// parse_closure on HANDLER_SCRIPT (common -- bad demos, missing commands,
// etc). We don't want any of those to panic the wasm isolate -- they
// should surface as readable HTTP 500s. So engine() returns Result and
// the cache is populated only on success. On failure each request retries
// and returns a fresh error body; cheap to keep retrying because the
// failure is deterministic (same embedded handler) and parse is the
// expensive bit anyway.
static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();

/// Lazily build the cached engine + parse the embedded handler. Returns
/// `Err(message)` if init or parse fails; the caller turns that into a
/// 500 response so wrangler dev / live workers don't crash on a bad
/// embedded handler.
///
/// Workspace shadow commands (ls, open, save, ...) are added LAST so
/// they take priority over Nu's stock declarations during eval.
pub(super) fn engine() -> std::result::Result<&'static Mutex<Engine>, String> {
    if let Some(e) = ENGINE.get() {
        return Ok(e);
    }
    let mut engine = Engine::new().map_err(|e| format!("Engine::new: {e}"))?;
    engine
        .add_custom_commands()
        .map_err(|e| format!("add_custom_commands: {e}"))?;
    engine
        .add_commands(vec![
            Box::new(nu_command::VfsLs),
            Box::new(nu_command::VfsOpen),
            Box::new(nu_command::VfsSave),
            Box::new(nu_command::VfsPathExists),
            Box::new(nu_command::VfsMkdir),
            Box::new(nu_command::VfsRm),
            Box::new(nu_command::VfsCp),
            Box::new(nu_command::VfsMv),
            Box::new(nu_command::VfsGlob),
            // `path self` shadowed because the stock impl needs a
            // working std::Path::is_absolute, which wasm32 lacks.
            Box::new(nu_command::VfsPathSelf),
            // `sleep` is the only os-gated command we shadow -- the
            // others (date now, format date, random integer, ...)
            // come from stock nu-command with `nu-command/js`
            // enabled (Cargo.toml `cloudflare` feature).
            Box::new(nu_command::Sleep),
        ])
        .map_err(|e| format!("add_commands (vfs shadows): {e}"))?;
    // Set $HTTP_NU const so stdlib modules (http, datastar, ...) can
    // reference $HTTP_NU.dev / .store / .topic at parse time. Desktop
    // sets this from CLI flags in src/main.rs; on CF the defaults are
    // correct (no --dev, no --store, no --topic).
    engine
        .set_http_nu_const(&crate::engine::HttpNuOptions::default())
        .map_err(|e| format!("set_http_nu_const: {e}"))?;
    engine
        .parse_closure(HANDLER_SCRIPT, None)
        .map_err(|e| format!("handler failed to parse:\n{e}"))?;
    // Race-tolerant: if another request initialised in parallel, drop
    // ours and return theirs. OnceLock guarantees only one ever wins.
    Ok(ENGINE.get_or_init(|| Mutex::new(engine)))
}

/// One Durable Object instance per user. The DO's storage.sql() backs a
/// per-user Workspace (cloudflare_shell_workspace::Workspace, our Rust port of
/// @cloudflare/shell's filesystem.ts).
///
/// Routing:
/// - `/_workspace/*` debug routes: hit the Workspace directly, bypass Nu.
///   Useful for verification with curl, used during development.
/// - everything else: falls through to the existing handler. Once the
///   Nu shadow-command wiring lands the handler will see Workspace files
///   via SnapshotVfs.
#[durable_object]
pub struct UserSpace {
    state: State,
    env: Env,
}

impl DurableObject for UserSpace {
    fn new(state: State, env: Env) -> Self {
        Self { state, env }
    }

    async fn fetch(&self, mut req: WorkerRequest) -> Result<Response> {
        // Hot-reload check: if the previous request wrote
        // HANDLER_SCRIPT_PATH (anywhere -- Nu shadow `save`, debug PUT),
        // re-parse the engine before serving anything else.
        self.maybe_reload_handler().await;

        let url = req.url()?;
        let path = url.path().to_string();
        // Strip the leading /{user_id}/ so debug routes can match a fixed
        // shape regardless of which user's DO they land in.
        let suffix = strip_user_prefix(&path);
        if suffix.starts_with("/_workspace/") {
            return self.workspace_debug(&suffix, &mut req).await;
        }

        // Reset per-request budgets before eval. Today: the sleep
        // call counter (CF defensive cap; see platform/sleep.rs).
        nu::nu_command::platform::sleep::reset_sleep_budget();

        // Preload the per-request snapshot from Workspace. Nu shadow
        // commands (ls/open/save/...) read it sync during eval through
        // the crate::vfs::Vfs trait. We keep our own Rc-clone of the
        // SnapshotVfs so we can drain pending writes/ops after eval.
        let ws = self.open_workspace()?;
        let snapshot = SnapshotVfs::load_from_workspace(&ws, 4, 1_500_000).await?;
        crate::vfs::install_vfs(Box::new(snapshot.clone()));

        let response = handler::handle(&mut req).await;

        // Drain pending writes + ops and flush back to Workspace. We do
        // this even if the handler errored so partial writes still
        // persist. Order: mkdir -> writes -> rm so a script that does
        // mkdir then save then rm leaves a consistent state.
        let writes = snapshot.drain_pending_writes();
        let ops = snapshot.drain_pending_ops();
        for op in &ops {
            if let snapshot_vfs::PendingOp::Mkdir(path) = op {
                let p = path.to_string_lossy().to_string();
                if let Err(e) = ws.mkdir(&p, MkdirOptions { recursive: true }).await {
                    worker::console_log!("flush mkdir {p} failed: {e:?}");
                }
            }
        }
        for (path, bytes) in writes {
            let p = path.to_string_lossy().to_string();
            // Nu shadow `save` doesn't carry MIME; pass None so the
            // Workspace falls back to its `application/octet-stream`
            // default (matches upstream `writeFileBytes`).
            if let Err(e) = ws.write_file_bytes(&p, &bytes, None).await {
                worker::console_log!("flush pending write {p} failed: {e:?}");
            }
        }
        for op in ops {
            if let snapshot_vfs::PendingOp::Rm(path) = op {
                let p = path.to_string_lossy().to_string();
                if let Err(e) = ws
                    .rm(
                        &p,
                        RmOptions {
                            recursive: true,
                            force: true,
                        },
                    )
                    .await
                {
                    worker::console_log!("flush rm {p} failed: {e:?}");
                }
            }
        }
        crate::vfs::drop_vfs();
        response
    }
}

/// Workspace `onChange` listener. Sets `HANDLER_RELOAD_PENDING` whenever
/// the handler script gets written or updated. Pure flag flip -- the
/// actual read+parse is async and happens at the start of the next
/// request (see `UserSpace::maybe_reload_handler`).
fn handler_reload_listener(event: WorkspaceChangeEvent) {
    use WorkspaceChangeType::*;
    if event.path == HANDLER_SCRIPT_PATH && matches!(event.kind, Create | Update) {
        HANDLER_RELOAD_PENDING.store(true, Ordering::SeqCst);
    }
}

impl UserSpace {
    fn open_workspace(&self) -> Result<Workspace> {
        let sql = self.state.storage().sql();
        let r2 = self.env.bucket("WORKSPACE_FILES").ok();
        let ws = Workspace::default(sql, r2)?;
        // Every Workspace this DO mints carries the reload listener, so
        // ALL write paths -- snapshot drain, debug PUT, future routes --
        // funnel through the same hot-reload signal.
        let cb: OnChange = Arc::new(handler_reload_listener);
        ws.set_on_change(cb);
        Ok(ws)
    }

    /// If a recent Workspace write touched `HANDLER_SCRIPT_PATH`, re-read
    /// it and swap into the cached engine. No-op when the flag isn't set
    /// (the common case). Errors are logged but don't fail the request:
    /// a bad reload should fall back to the previously-parsed handler.
    async fn maybe_reload_handler(&self) {
        if !HANDLER_RELOAD_PENDING.swap(false, Ordering::SeqCst) {
            return;
        }
        // Build a Workspace without the listener -- we're reading, not
        // writing, so no events would fire anyway, but skipping the
        // listener install also avoids any future cross-talk.
        let sql = self.state.storage().sql();
        let r2 = self.env.bucket("WORKSPACE_FILES").ok();
        let ws = match Workspace::default(sql, r2) {
            Ok(ws) => ws,
            Err(e) => {
                worker::console_warn!("handler reload: open_workspace failed: {e:?}");
                return;
            }
        };
        let script = match ws.read_file(HANDLER_SCRIPT_PATH).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                worker::console_warn!(
                    "handler reload: {HANDLER_SCRIPT_PATH} not found in Workspace"
                );
                return;
            }
            Err(e) => {
                worker::console_warn!("handler reload: read failed: {e:?}");
                return;
            }
        };
        let engine_handle = match engine() {
            Ok(e) => e,
            Err(e) => {
                worker::console_warn!("handler reload: engine init failed: {e}");
                return;
            }
        };
        let mut engine_guard = match engine_handle.lock() {
            Ok(g) => g,
            Err(_) => {
                worker::console_warn!("handler reload: engine mutex poisoned");
                return;
            }
        };
        match engine_guard.parse_closure(&script, None) {
            Ok(_) => worker::console_log!("handler reloaded from {HANDLER_SCRIPT_PATH}"),
            Err(e) => worker::console_warn!("handler reload parse failed: {e}"),
        }
    }

    async fn workspace_debug(&self, suffix: &str, req: &mut WorkerRequest) -> Result<Response> {
        let ws = self.open_workspace()?;
        let url = req.url()?;
        let query = url.query_pairs();
        let path_param = query
            .into_iter()
            .find(|(k, _)| k == "path")
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| "/".to_string());

        match suffix {
            "/_workspace/ls" => {
                let entries = ws.read_dir(&path_param).await?.unwrap_or_default();
                let body = serde_json::to_string(&entries)
                    .map_err(|e| worker::Error::RustError(format!("json: {e}")))?;
                let mut resp = Response::ok(body)?;
                let _ = resp.headers_mut().set("Content-Type", "application/json");
                Ok(resp)
            }
            "/_workspace/stat" => {
                let stat = ws.lstat(&path_param).await?;
                let body = match stat {
                    Some(s) => format!(
                        r#"{{"kind":"{kind}","size":{size},"modified_at":{mt},"mime_type":"{mt2}"}}"#,
                        kind = match s.kind {
                            EntryType::File => "file",
                            EntryType::Directory => "directory",
                            EntryType::Symlink => "symlink",
                        },
                        size = s.size,
                        mt = s.modified_at,
                        mt2 = s.mime_type,
                    ),
                    None => "null".to_string(),
                };
                let mut resp = Response::ok(body)?;
                let _ = resp.headers_mut().set("Content-Type", "application/json");
                Ok(resp)
            }
            "/_workspace/cat" => match ws.read_file_bytes(&path_param).await? {
                Some(bytes) => Response::from_bytes(bytes),
                None => Response::error("not found", 404),
            },
            "/_workspace/put" => {
                // Honor the request's Content-Type so files written from
                // browser uploads land in Workspace with the right
                // `mime_type` (and serve back via `.static` with the
                // correct Content-Type).
                let mime = req.headers().get("Content-Type").ok().flatten();
                let body = req.bytes().await.unwrap_or_default();
                ws.write_file_bytes(&path_param, &body, mime.as_deref())
                    .await?;
                Response::ok("ok")
            }
            "/_workspace/rm" => {
                ws.rm(
                    &path_param,
                    RmOptions {
                        recursive: true,
                        force: true,
                    },
                )
                .await?;
                Response::ok("ok")
            }
            "/_workspace/mkdir" => {
                ws.mkdir(&path_param, MkdirOptions { recursive: true })
                    .await?;
                Response::ok("ok")
            }
            "/_workspace/conformance" => {
                let sql = self.state.storage().sql();
                let r2 = self.env.bucket("WORKSPACE_FILES").ok();
                cloudflare_shell_workspace::run_conformance(sql, r2).await
            }
            _ => Response::error(format!("unknown debug route: {suffix}"), 404),
        }
    }
}

/// Strip `/<user_id>` from the URL path so closures see paths mounted
/// at root, matching desktop. Used by `fetch` (for debug-route
/// dispatch) and by `cf::request::worker_request_to_http_nu` (for
/// `Request.path` population).
///
/// Strip the explicit per-user prefix from a path so the handler sees a
/// path that's identical whether or not the caller used per-user routing.
///
/// The CF target supports an optional `/u/<user>/` URL prefix for
/// per-user DurableObject isolation. Everything else goes to the
/// "default" DO, so root-relative URLs in demos (`/datastar@1.0.1.js`,
/// `/static/...`, `/2048/move`) work like desktop -- no path mangling.
///
/// "/u/alice/foo"        -> "/foo"
/// "/u/alice"             -> "/"
/// "/foo"                 -> "/foo"   (default DO, no strip)
/// "/"                    -> "/"
pub(super) fn strip_user_prefix(path: &str) -> String {
    if let Some(after) = path.strip_prefix("/u/") {
        let mut parts = after.splitn(2, '/');
        let _user = parts.next();
        match parts.next() {
            Some(rest) if !rest.is_empty() => format!("/{rest}"),
            _ => "/".to_string(),
        }
    } else {
        path.to_string()
    }
}

/// Pull the user_id from an explicit `/u/<user>/...` prefix, or fall
/// back to `"default"` for the global namespace.
///
/// `/u/alice/posts`     -> "alice"
/// `/u/alice`           -> "alice"
/// `/foo`               -> "default"
/// `/`                  -> "default"
/// `/datastar@1.0.1.js` -> "default"  (no longer mis-parsed as a user)
fn user_id_from_path(path: &str) -> &str {
    if let Some(after) = path.strip_prefix("/u/") {
        let id = after.split('/').next().unwrap_or("");
        if !id.is_empty() {
            return id;
        }
    }
    "default"
}

#[worker::event(fetch)]
async fn fetch(req: WorkerRequest, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let user_id = user_id_from_path(url.path()).to_string();

    let namespace = env.durable_object("USER_SPACE")?;
    let stub = namespace.id_from_name(&user_id)?.get_stub()?;
    stub.fetch_with_request(req).await
}
