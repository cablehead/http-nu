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

mod commands;
mod handler;
mod request;
mod response;
mod vfs;
mod workspace;

use std::sync::{Mutex, OnceLock};

use worker::{
    durable_object, wasm_bindgen, Context, DurableObject, Env, Request as WorkerRequest, Response,
    Result, State,
};

use crate::engine::Engine;
use vfs::SnapshotVfs;

const HANDLER_SCRIPT: &str = include_str!(env!("CF_HANDLER_PATH"));

// Per-isolate engine cache. Each DO instance runs in its own wasm
// isolate, so this OnceLock is effectively per-user: alice's UserSpace
// isolate caches alice's engine, bob's caches bob's. Until the handler
// becomes user-specific we still share one HANDLER_SCRIPT, but state
// the engine builds up (parsed AST, plugin signatures) is isolated.
static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();

/// Lazily build the cached engine + parse the embedded handler. Panics
/// on first-use if engine init or handler parsing fails;
/// console_error_panic_hook surfaces a readable error to wrangler logs.
///
/// Workspace shadow commands (ls, open, save) are added LAST so they
/// take priority over Nu's stock declarations during eval.
pub(super) fn engine() -> &'static Mutex<Engine> {
    ENGINE.get_or_init(|| {
        let mut engine = Engine::new().expect("Engine::new failed");
        engine
            .add_custom_commands()
            .expect("add_custom_commands failed");
        engine
            .add_commands(vec![
                Box::new(commands::VfsLs),
                Box::new(commands::VfsOpen),
                Box::new(commands::VfsSave),
                Box::new(commands::VfsPathExists),
                Box::new(commands::VfsMkdir),
                Box::new(commands::VfsRm),
                Box::new(commands::VfsCp),
                Box::new(commands::VfsMv),
                Box::new(commands::VfsGlob),
                // `path self` shadowed because the stock impl needs a
                // working std::Path::is_absolute, which wasm32 lacks.
                Box::new(commands::VfsPathSelf),
                // `sleep` is the only os-gated command we shadow -- the
                // others (date now, format date, random integer, ...)
                // come from stock nu-command with `nu-command/js`
                // enabled (Cargo.toml `cloudflare` feature).
                Box::new(commands::Sleep),
            ])
            .expect("add_commands (vfs shadows) failed");
        engine
            .parse_closure(HANDLER_SCRIPT, None)
            .expect("handler failed to parse");
        Mutex::new(engine)
    })
}

/// One Durable Object instance per user. The DO's storage.sql() backs a
/// per-user Workspace (crate::cf::workspace, our Rust port of
/// @cloudflare/shell's filesystem).
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
        let url = req.url()?;
        let path = url.path().to_string();
        // Strip the leading /{user_id}/ so debug routes can match a fixed
        // shape regardless of which user's DO they land in.
        let suffix = strip_user_prefix(&path);
        if suffix.starts_with("/_workspace/") {
            return self.workspace_debug(&suffix, &mut req).await;
        }

        // Preload the per-request snapshot from Workspace. Nu shadow
        // commands (ls/open/save/...) read it sync during eval through
        // the cf::vfs::Vfs trait. We keep our own Rc-clone of the
        // SnapshotVfs so we can drain pending writes/ops after eval.
        let ws = self.open_workspace()?;
        let snapshot = SnapshotVfs::load_from_workspace(&ws, 4, 1_500_000).await?;
        vfs::install_vfs(Box::new(snapshot.clone()));

        let response = handler::handle(&mut req).await;

        // Drain pending writes + ops and flush back to Workspace. We do
        // this even if the handler errored so partial writes still
        // persist. Order: mkdir -> writes -> rm so a script that does
        // mkdir then save then rm leaves a consistent state.
        let writes = snapshot.drain_pending_writes();
        let ops = snapshot.drain_pending_ops();
        for op in &ops {
            if let vfs::PendingOp::Mkdir(path) = op {
                let p = path.to_string_lossy().to_string();
                if let Err(e) = ws
                    .mkdir(&p, workspace::MkdirOptions { recursive: true })
                    .await
                {
                    worker::console_log!("flush mkdir {p} failed: {e:?}");
                }
            }
        }
        for (path, bytes) in writes {
            let p = path.to_string_lossy().to_string();
            if let Err(e) = ws.write_file_bytes(&p, &bytes).await {
                worker::console_log!("flush pending write {p} failed: {e:?}");
            }
        }
        for op in ops {
            if let vfs::PendingOp::Rm(path) = op {
                let p = path.to_string_lossy().to_string();
                if let Err(e) = ws
                    .rm(
                        &p,
                        workspace::RmOptions {
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
        vfs::drop_vfs();
        response
    }
}

impl UserSpace {
    fn open_workspace(&self) -> Result<workspace::Workspace> {
        let sql = self.state.storage().sql();
        let r2 = self.env.bucket("WORKSPACE_FILES").ok();
        workspace::Workspace::default(sql, r2)
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
                            workspace::EntryType::File => "file",
                            workspace::EntryType::Directory => "directory",
                            workspace::EntryType::Symlink => "symlink",
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
                let body = req.bytes().await.unwrap_or_default();
                ws.write_file_bytes(&path_param, &body).await?;
                Response::ok("ok")
            }
            "/_workspace/rm" => {
                ws.rm(
                    &path_param,
                    workspace::RmOptions {
                        recursive: true,
                        force: true,
                    },
                )
                .await?;
                Response::ok("ok")
            }
            "/_workspace/mkdir" => {
                ws.mkdir(&path_param, workspace::MkdirOptions { recursive: true })
                    .await?;
                Response::ok("ok")
            }
            _ => Response::error(format!("unknown debug route: {suffix}"), 404),
        }
    }
}

/// "/alice/foo"          -> "/foo"
/// "/alice"              -> "/"
/// "/"                   -> "/"
fn strip_user_prefix(path: &str) -> String {
    let mut parts = path.splitn(3, '/');
    parts.next(); // empty before leading /
    parts.next(); // user_id
    match parts.next() {
        Some(rest) if !rest.is_empty() => format!("/{rest}"),
        _ => "/".to_string(),
    }
}

/// Pull the user_id from the URL's first path segment.
///
/// "/alice/posts"       -> "alice"
/// "/alice"             -> "alice"
/// "/"                  -> "default"
/// "/datastar@1.0.1.js" -> still maps to a user named "datastar@1.0.1.js"
///   today; that's fine for the MVP because the segment is opaque and
///   the per-isolate engine cache works either way. Future work moves
///   well-known asset routes to a separate Worker namespace.
fn user_id_from_path(path: &str) -> &str {
    path.split('/')
        .nth(1)
        .filter(|s| !s.is_empty())
        .unwrap_or("default")
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
