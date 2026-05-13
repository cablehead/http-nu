//! Optional HTTP surface mirroring the demos' route shape, but served
//! by the FS-RPC server directly so callers can skip the
//! service-binding hop entirely.
//!
//! Routes (same shape as demo-js / demo-rust so smoke + bench reuse
//! the URL grammar):
//!
//! | Method | Path                  | Op             |
//! |--------|-----------------------|----------------|
//! | GET    | `/fs/:ns/:path`       | `read_file`    |
//! | PUT    | `/fs/:ns/:path`       | `write_file`   |
//! | DELETE | `/fs/:ns/:path`       | `rm`           |
//! | GET    | `/stat/:ns/:path`     | `stat`         |
//! | GET    | `/list/:ns/:path`     | `list`         |
//! | POST   | `/mkdir/:ns/:path`    | `mkdir`        |
//!
//! Why this exists at all: the demos go through `service-binding ->
//! WorkerEntrypoint -> RPC dispatch -> DO`. Hitting the server's HTTP
//! routes goes `HTTP -> #[event(fetch)] -> DO`, one fewer hop. The
//! delta is the binding + RPC dispatch cost. The bench matrix exposes
//! both so the cost is measurable.
//!
//! Auth uses the same `SHELL_FS_TOKEN` env var as the RPC path; if set,
//! every HTTP request must carry `Authorization: Bearer <token>`. If
//! unset, no auth check runs and the routes are open. See
//! `server/README.md` for the threat model.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use cloudflare_shell_rpc_types::{ListReq, MkdirReq, ReadFileReq, RmReq, StatReq, WriteFileReq};
use percent_encoding::percent_decode_str;
use worker::{Env, Method, Request, Response, Result, Stub};

use crate::wire::{build_request, call_do};

const DO_BINDING: &str = "SHELL_FS_DO";
const TOKEN_ENV: &str = "SHELL_FS_TOKEN";

/// True if the request URL path matches one of the FS routes this
/// module handles. The `#[event(fetch)]` entry uses this to decide
/// whether to dispatch into `handle` or fall through to the health
/// banner.
pub fn handles(path: &str) -> bool {
    path.starts_with("/fs/")
        || path.starts_with("/stat/")
        || path.starts_with("/list/")
        || path.starts_with("/mkdir/")
}

/// Dispatch a parsed HTTP request to the matching FS op.
pub async fn handle(req: &mut Request, env: Env) -> Result<Response> {
    if let Err(resp) = check_auth(req, &env) {
        return resp;
    }

    let url = req.url()?;
    let path = url.path().to_string();
    let method = req.method();

    if let Some(p) = parse(&path, "/fs/") {
        return match method {
            Method::Get => http_read_file(&env, &p).await,
            Method::Put => http_write_file(req, &env, &p).await,
            Method::Delete => http_rm(req, &env, &p).await,
            _ => Response::error("method not allowed on /fs/", 405),
        };
    }
    if let Some(p) = parse(&path, "/stat/") {
        return http_stat(&env, &p).await;
    }
    if let Some(p) = parse(&path, "/list/") {
        return http_list(&env, &p).await;
    }
    if let Some(p) = parse(&path, "/mkdir/") {
        if !matches!(method, Method::Post) {
            return Response::error("mkdir is POST", 405);
        }
        let recursive = req.url()?.query_pairs().any(|(k, _)| k == "recursive");
        return http_mkdir(&env, &p, recursive).await;
    }
    Response::error("not found", 404)
}

struct Parsed {
    namespace: String,
    path: String,
}

fn parse(path: &str, prefix: &str) -> Option<Parsed> {
    let stripped = path.strip_prefix(prefix)?;
    let slash = stripped.find('/')?;
    let namespace = &stripped[..slash];
    let raw = &stripped[slash..];
    if namespace.is_empty() || raw.is_empty() {
        return None;
    }
    // Same decode story as the demos: paths with %2F come through
    // intact and we want the decoded form.
    let fs_path = percent_decode_str(raw).decode_utf8().ok()?.into_owned();
    Some(Parsed {
        namespace: namespace.to_string(),
        path: fs_path,
    })
}

/// If `SHELL_FS_TOKEN` is set, every request must carry
/// `Authorization: Bearer <token>`. Otherwise no-op. Returns `Err` of
/// the error response to send.
fn check_auth(req: &Request, env: &Env) -> std::result::Result<(), Result<Response>> {
    let Ok(token) = env.var(TOKEN_ENV).map(|v| v.to_string()) else {
        return Ok(());
    };
    let supplied = req
        .headers()
        .get("authorization")
        .ok()
        .flatten()
        .and_then(|h| h.strip_prefix("Bearer ").map(str::to_string));
    if supplied.as_deref() == Some(token.as_str()) {
        Ok(())
    } else {
        Err(Response::error(
            "ENOENT: authentication required (Authorization: Bearer <token>)",
            401,
        ))
    }
}

async fn do_stub(env: &Env, namespace: &str) -> Result<Stub> {
    let ns = env.durable_object(DO_BINDING)?;
    ns.id_from_name(namespace)?.get_stub()
}

async fn http_read_file(env: &Env, p: &Parsed) -> Result<Response> {
    let body = ReadFileReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/read_file", &body)?;
    let resp: cloudflare_shell_rpc_types::ReadFileResp = call_do(&stub, internal).await?;
    match resp.data {
        Some(b64) => {
            let bytes = B64
                .decode(b64.as_bytes())
                .map_err(|e| worker::Error::RustError(format!("base64 decode: {e}")))?;
            Response::from_bytes(bytes)
        }
        None => Response::error("not found", 404),
    }
}

async fn http_write_file(req: &mut Request, env: &Env, p: &Parsed) -> Result<Response> {
    let raw = req.bytes().await?;
    let mime = req.headers().get("content-type").ok().flatten();
    let body = WriteFileReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        data: B64.encode(&raw),
        mime_type: mime,
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/write_file", &body)?;
    let _ack: cloudflare_shell_rpc_types::Ack = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "ok": true, "bytes": raw.len() }))
}

async fn http_rm(req: &mut Request, env: &Env, p: &Parsed) -> Result<Response> {
    let url = req.url()?;
    let recursive = url.query_pairs().any(|(k, _)| k == "recursive");
    let force = url.query_pairs().any(|(k, _)| k == "force");
    let body = RmReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        recursive,
        force,
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/rm", &body)?;
    let _ack: cloudflare_shell_rpc_types::Ack = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "ok": true }))
}

async fn http_stat(env: &Env, p: &Parsed) -> Result<Response> {
    let body = StatReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/stat", &body)?;
    let resp: cloudflare_shell_rpc_types::StatResp = call_do(&stub, internal).await?;
    let status = if resp.stat.is_none() { 404 } else { 200 };
    Response::from_json(&serde_json::json!({ "stat": resp.stat })).map(|r| r.with_status(status))
}

async fn http_list(env: &Env, p: &Parsed) -> Result<Response> {
    let body = ListReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/list", &body)?;
    let resp: cloudflare_shell_rpc_types::ListResp = call_do(&stub, internal).await?;
    let status = if resp.entries.is_none() { 404 } else { 200 };
    Response::from_json(&serde_json::json!({ "entries": resp.entries }))
        .map(|r| r.with_status(status))
}

async fn http_mkdir(env: &Env, p: &Parsed, recursive: bool) -> Result<Response> {
    let body = MkdirReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        recursive,
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/mkdir", &body)?;
    let _ack: cloudflare_shell_rpc_types::Ack = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "ok": true }))
}
