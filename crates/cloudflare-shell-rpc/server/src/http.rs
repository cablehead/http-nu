//! Optional HTTP surface mirroring the demos' route shape, but served
//! by the FS-RPC server directly so callers can skip the
//! service-binding hop entirely.
//!
//! Routes (same shape as demo-js / demo-rust so smoke + bench reuse
//! the URL grammar):
//!
//! | Method | Path                              | Op                   |
//! |--------|-----------------------------------|----------------------|
//! | GET    | `/fs/:ns/:path`                   | `read_file`          |
//! | PUT    | `/fs/:ns/:path`                   | `write_file`         |
//! | DELETE | `/fs/:ns/:path?recursive&force`   | `rm`                 |
//! | POST   | `/append/:ns/:path`               | `append_file`        |
//! | POST   | `/delete_file/:ns/:path`          | `delete_file`        |
//! | GET    | `/stat/:ns/:path`                 | `stat`               |
//! | GET    | `/lstat/:ns/:path`                | `lstat`              |
//! | GET    | `/exists/:ns/:path`               | `exists`             |
//! | GET    | `/file_exists/:ns/:path`          | `file_exists`        |
//! | GET    | `/list/:ns/:path`                 | `list`               |
//! | POST   | `/mkdir/:ns/:path?recursive`      | `mkdir`              |
//! | POST   | `/cp/:ns/:src?dst&recursive`      | `cp`                 |
//! | POST   | `/mv/:ns/:src?dst`                | `mv`                 |
//! | POST   | `/symlink/:ns/:link?target`       | `symlink`            |
//! | GET    | `/readlink/:ns/:path`             | `readlink`           |
//! | GET    | `/realpath/:ns/:path`             | `realpath`           |
//! | GET    | `/glob/:ns?pattern`               | `glob`               |
//! | GET    | `/info/:ns`                       | `workspace_info`     |
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
use cloudflare_shell_rpc_types::{
    Ack, AppendFileReq, CpReq, DeleteFileReq, DeleteFileResp, ExistsReq, ExistsResp, FileExistsReq,
    FileExistsResp, GlobReq, GlobResp, ListReq, LstatReq, LstatResp, MkdirReq, MvReq, ReadFileReq,
    ReadlinkReq, ReadlinkResp, RealpathReq, RealpathResp, RmReq, StatReq, SymlinkReq,
    WorkspaceInfoReq, WorkspaceInfoResp, WriteFileReq,
};
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
        || path.starts_with("/lstat/")
        || path.starts_with("/exists/")
        || path.starts_with("/file_exists/")
        || path.starts_with("/append/")
        || path.starts_with("/delete_file/")
        || path.starts_with("/cp/")
        || path.starts_with("/mv/")
        || path.starts_with("/symlink/")
        || path.starts_with("/readlink/")
        || path.starts_with("/realpath/")
        || path.starts_with("/glob/")
        || path == "/glob"
        || path.starts_with("/info/")
        || path == "/info"
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
    if let Some(p) = parse(&path, "/lstat/") {
        return http_lstat(&env, &p).await;
    }
    if let Some(p) = parse(&path, "/exists/") {
        return http_exists(&env, &p).await;
    }
    if let Some(p) = parse(&path, "/file_exists/") {
        return http_file_exists(&env, &p).await;
    }
    if let Some(p) = parse(&path, "/readlink/") {
        return http_readlink(&env, &p).await;
    }
    if let Some(p) = parse(&path, "/realpath/") {
        return http_realpath(&env, &p).await;
    }
    if let Some(p) = parse(&path, "/append/") {
        if !matches!(method, Method::Post) {
            return Response::error("append is POST", 405);
        }
        return http_append_file(req, &env, &p).await;
    }
    if let Some(p) = parse(&path, "/delete_file/") {
        if !matches!(method, Method::Post) {
            return Response::error("delete_file is POST", 405);
        }
        return http_delete_file(&env, &p).await;
    }
    if let Some(p) = parse(&path, "/cp/") {
        if !matches!(method, Method::Post) {
            return Response::error("cp is POST", 405);
        }
        let url = req.url()?;
        let dst = url
            .query_pairs()
            .find(|(k, _)| k == "dst")
            .map(|(_, v)| v.into_owned());
        let recursive = url.query_pairs().any(|(k, _)| k == "recursive");
        let Some(dst) = dst else {
            return Response::error("cp requires ?dst=<path>", 400);
        };
        return http_cp(&env, &p, &dst, recursive).await;
    }
    if let Some(p) = parse(&path, "/mv/") {
        if !matches!(method, Method::Post) {
            return Response::error("mv is POST", 405);
        }
        let url = req.url()?;
        let dst = url
            .query_pairs()
            .find(|(k, _)| k == "dst")
            .map(|(_, v)| v.into_owned());
        let Some(dst) = dst else {
            return Response::error("mv requires ?dst=<path>", 400);
        };
        return http_mv(&env, &p, &dst).await;
    }
    if let Some(p) = parse(&path, "/symlink/") {
        if !matches!(method, Method::Post) {
            return Response::error("symlink is POST", 405);
        }
        let url = req.url()?;
        let target = url
            .query_pairs()
            .find(|(k, _)| k == "target")
            .map(|(_, v)| v.into_owned());
        let Some(target) = target else {
            return Response::error("symlink requires ?target=<path>", 400);
        };
        return http_symlink(&env, &p, &target).await;
    }
    if let Some(ns) = parse_ns(&path, "/glob") {
        let url = req.url()?;
        let pattern = url
            .query_pairs()
            .find(|(k, _)| k == "pattern")
            .map(|(_, v)| v.into_owned());
        let Some(pattern) = pattern else {
            return Response::error("glob requires ?pattern=<glob>", 400);
        };
        return http_glob(&env, &ns, &pattern).await;
    }
    if let Some(ns) = parse_ns(&path, "/info") {
        return http_workspace_info(&env, &ns).await;
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

/// `parse` variant for routes that take a namespace but no FS path
/// (today: `/glob/<ns>`, `/info/<ns>`). Accepts the prefix with or
/// without a trailing slash. Returns the namespace.
fn parse_ns(path: &str, prefix: &str) -> Option<String> {
    let stripped = path.strip_prefix(prefix)?;
    let ns = stripped.trim_start_matches('/').trim_end_matches('/');
    if ns.is_empty() {
        return None;
    }
    Some(ns.to_string())
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

async fn http_lstat(env: &Env, p: &Parsed) -> Result<Response> {
    let body = LstatReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/lstat", &body)?;
    let resp: LstatResp = call_do(&stub, internal).await?;
    let status = if resp.stat.is_none() { 404 } else { 200 };
    Response::from_json(&serde_json::json!({ "stat": resp.stat })).map(|r| r.with_status(status))
}

async fn http_exists(env: &Env, p: &Parsed) -> Result<Response> {
    let body = ExistsReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/exists", &body)?;
    let resp: ExistsResp = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "exists": resp.exists }))
}

async fn http_file_exists(env: &Env, p: &Parsed) -> Result<Response> {
    let body = FileExistsReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/file_exists", &body)?;
    let resp: FileExistsResp = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "exists": resp.exists }))
}

async fn http_readlink(env: &Env, p: &Parsed) -> Result<Response> {
    let body = ReadlinkReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/readlink", &body)?;
    let resp: ReadlinkResp = call_do(&stub, internal).await?;
    let status = if resp.target.is_none() { 404 } else { 200 };
    Response::from_json(&serde_json::json!({ "target": resp.target }))
        .map(|r| r.with_status(status))
}

async fn http_realpath(env: &Env, p: &Parsed) -> Result<Response> {
    let body = RealpathReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/realpath", &body)?;
    let resp: RealpathResp = call_do(&stub, internal).await?;
    let status = if resp.path.is_none() { 404 } else { 200 };
    Response::from_json(&serde_json::json!({ "path": resp.path })).map(|r| r.with_status(status))
}

async fn http_append_file(req: &mut Request, env: &Env, p: &Parsed) -> Result<Response> {
    let raw = req.bytes().await?;
    let body = AppendFileReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        data: B64.encode(&raw),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/append_file", &body)?;
    let _ack: Ack = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "ok": true, "bytes": raw.len() }))
}

async fn http_delete_file(env: &Env, p: &Parsed) -> Result<Response> {
    let body = DeleteFileReq {
        namespace: p.namespace.clone(),
        path: p.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &p.namespace).await?;
    let internal = build_request("/delete_file", &body)?;
    let resp: DeleteFileResp = call_do(&stub, internal).await?;
    let status = if resp.removed { 200 } else { 404 };
    Response::from_json(&serde_json::json!({ "removed": resp.removed }))
        .map(|r| r.with_status(status))
}

async fn http_cp(env: &Env, src: &Parsed, dst: &str, recursive: bool) -> Result<Response> {
    let body = CpReq {
        namespace: src.namespace.clone(),
        src: src.path.clone(),
        dst: dst.to_string(),
        recursive,
        auth: None,
    };
    let stub = do_stub(env, &src.namespace).await?;
    let internal = build_request("/cp", &body)?;
    let _ack: Ack = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "ok": true }))
}

async fn http_mv(env: &Env, src: &Parsed, dst: &str) -> Result<Response> {
    let body = MvReq {
        namespace: src.namespace.clone(),
        src: src.path.clone(),
        dst: dst.to_string(),
        auth: None,
    };
    let stub = do_stub(env, &src.namespace).await?;
    let internal = build_request("/mv", &body)?;
    let _ack: Ack = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "ok": true }))
}

async fn http_symlink(env: &Env, link: &Parsed, target: &str) -> Result<Response> {
    let body = SymlinkReq {
        namespace: link.namespace.clone(),
        target: target.to_string(),
        link_path: link.path.clone(),
        auth: None,
    };
    let stub = do_stub(env, &link.namespace).await?;
    let internal = build_request("/symlink", &body)?;
    let _ack: Ack = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "ok": true }))
}

async fn http_glob(env: &Env, namespace: &str, pattern: &str) -> Result<Response> {
    let body = GlobReq {
        namespace: namespace.to_string(),
        pattern: pattern.to_string(),
        auth: None,
    };
    let stub = do_stub(env, namespace).await?;
    let internal = build_request("/glob", &body)?;
    let resp: GlobResp = call_do(&stub, internal).await?;
    Response::from_json(&serde_json::json!({ "paths": resp.paths }))
}

async fn http_workspace_info(env: &Env, namespace: &str) -> Result<Response> {
    let body = WorkspaceInfoReq {
        namespace: namespace.to_string(),
        auth: None,
    };
    let stub = do_stub(env, namespace).await?;
    let internal = build_request("/workspace_info", &body)?;
    let resp: WorkspaceInfoResp = call_do(&stub, internal).await?;
    Response::from_json(&resp)
}
