//! Rust Worker demo for `cloudflare-shell-rpc`.
//!
//! Mirrors `demo-js`'s HTTP surface so `cf:fs:smoke:rust` can reuse
//! the same curl sequence. Serves as the integration test for
//! `cloudflare-shell-rpc-client`.

#![cfg(target_arch = "wasm32")]

use cloudflare_shell_rpc_client::{ShellFs, ShellFsService};
use worker::*;

const BINDING: &str = "SHELL_FS";

#[event(fetch)]
async fn fetch(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();

    let url = req.url()?;
    let method = req.method();
    let path = url.path().to_string();

    // If the consumer has SHELL_FS_TOKEN configured (via wrangler vars
    // or a secret), attach it to every call. Server only enforces if
    // its own SHELL_FS_TOKEN env is set; otherwise the field is ignored.
    let fs: ShellFsService = env.service(BINDING)?.into();
    let fs = match env.var("SHELL_FS_TOKEN") {
        Ok(token) => fs.with_auth(token.to_string()),
        Err(_) => fs,
    };

    match dispatch(&fs, &method, &path, &url, req).await {
        Ok(resp) => Ok(resp),
        Err(e) => {
            let msg = e.to_string();
            // Map ENOENT-on-read to a 404 so the smoke test sees the
            // same shape as demo-js. Other errors stay 500.
            let status = if msg.starts_with("ENOENT") { 404 } else { 500 };
            Response::error(msg, status)
        }
    }
}

async fn dispatch(
    fs: &ShellFsService,
    method: &Method,
    path: &str,
    url: &Url,
    mut req: Request,
) -> Result<Response> {
    if path == "/" {
        return Response::ok(BANNER);
    }

    if let Some(parsed) = parse("/fs/", path) {
        return match *method {
            Method::Get => {
                let bytes = fs.read_file(&parsed.namespace, &parsed.path).await?;
                match bytes {
                    Some(b) => Response::from_bytes(b),
                    None => Response::error("not found", 404),
                }
            }
            Method::Put => {
                let body = req.bytes().await?;
                let mime = req.headers().get("content-type").ok().flatten();
                fs.write_file(&parsed.namespace, &parsed.path, &body, mime.as_deref())
                    .await?;
                Response::from_json(&serde_json::json!({ "ok": true, "bytes": body.len() }))
            }
            Method::Delete => {
                let recursive = url.query_pairs().any(|(k, _)| k == "recursive");
                let force = url.query_pairs().any(|(k, _)| k == "force");
                fs.rm(&parsed.namespace, &parsed.path, recursive, force)
                    .await?;
                Response::from_json(&serde_json::json!({ "ok": true }))
            }
            _ => Response::error(format!("method {method:?} not allowed on /fs/"), 405),
        };
    }

    if let Some(parsed) = parse("/stat/", path) {
        let stat = fs.stat(&parsed.namespace, &parsed.path).await?;
        let status = if stat.is_none() { 404 } else { 200 };
        return Response::from_json(&serde_json::json!({ "stat": stat }))
            .map(|r| r.with_status(status));
    }

    if let Some(parsed) = parse("/list/", path) {
        let entries = fs.list(&parsed.namespace, &parsed.path).await?;
        let status = if entries.is_none() { 404 } else { 200 };
        return Response::from_json(&serde_json::json!({ "entries": entries }))
            .map(|r| r.with_status(status));
    }

    if let Some(parsed) = parse("/mkdir/", path) {
        if !matches!(method, Method::Post) {
            return Response::error("mkdir is POST", 405);
        }
        let recursive = url.query_pairs().any(|(k, _)| k == "recursive");
        fs.mkdir(&parsed.namespace, &parsed.path, recursive).await?;
        return Response::from_json(&serde_json::json!({ "ok": true }));
    }

    if let Some(parsed) = parse("/lstat/", path) {
        let stat = fs.lstat(&parsed.namespace, &parsed.path).await?;
        let status = if stat.is_none() { 404 } else { 200 };
        return Response::from_json(&serde_json::json!({ "stat": stat }))
            .map(|r| r.with_status(status));
    }

    if let Some(parsed) = parse("/exists/", path) {
        let exists = fs.exists(&parsed.namespace, &parsed.path).await?;
        return Response::from_json(&serde_json::json!({ "exists": exists }));
    }

    if let Some(parsed) = parse("/file_exists/", path) {
        let exists = fs.file_exists(&parsed.namespace, &parsed.path).await?;
        return Response::from_json(&serde_json::json!({ "exists": exists }));
    }

    if let Some(parsed) = parse("/readlink/", path) {
        let target = fs.readlink(&parsed.namespace, &parsed.path).await?;
        let status = if target.is_none() { 404 } else { 200 };
        return Response::from_json(&serde_json::json!({ "target": target }))
            .map(|r| r.with_status(status));
    }

    if let Some(parsed) = parse("/realpath/", path) {
        let resolved = fs.realpath(&parsed.namespace, &parsed.path).await?;
        let status = if resolved.is_none() { 404 } else { 200 };
        return Response::from_json(&serde_json::json!({ "path": resolved }))
            .map(|r| r.with_status(status));
    }

    if let Some(parsed) = parse("/append/", path) {
        if !matches!(method, Method::Post) {
            return Response::error("append is POST", 405);
        }
        let body = req.bytes().await?;
        fs.append_file(&parsed.namespace, &parsed.path, &body)
            .await?;
        return Response::from_json(&serde_json::json!({ "ok": true, "bytes": body.len() }));
    }

    if let Some(parsed) = parse("/delete_file/", path) {
        if !matches!(method, Method::Post) {
            return Response::error("delete_file is POST", 405);
        }
        let removed = fs.delete_file(&parsed.namespace, &parsed.path).await?;
        let status = if removed { 200 } else { 404 };
        return Response::from_json(&serde_json::json!({ "removed": removed }))
            .map(|r| r.with_status(status));
    }

    if let Some(parsed) = parse("/cp/", path) {
        if !matches!(method, Method::Post) {
            return Response::error("cp is POST", 405);
        }
        let dst = url
            .query_pairs()
            .find(|(k, _)| k == "dst")
            .map(|(_, v)| v.into_owned());
        let Some(dst) = dst else {
            return Response::error("cp requires ?dst=<path>", 400);
        };
        let recursive = url.query_pairs().any(|(k, _)| k == "recursive");
        fs.cp(&parsed.namespace, &parsed.path, &dst, recursive)
            .await?;
        return Response::from_json(&serde_json::json!({ "ok": true }));
    }

    if let Some(parsed) = parse("/mv/", path) {
        if !matches!(method, Method::Post) {
            return Response::error("mv is POST", 405);
        }
        let dst = url
            .query_pairs()
            .find(|(k, _)| k == "dst")
            .map(|(_, v)| v.into_owned());
        let Some(dst) = dst else {
            return Response::error("mv requires ?dst=<path>", 400);
        };
        fs.mv(&parsed.namespace, &parsed.path, &dst).await?;
        return Response::from_json(&serde_json::json!({ "ok": true }));
    }

    if let Some(parsed) = parse("/symlink/", path) {
        if !matches!(method, Method::Post) {
            return Response::error("symlink is POST", 405);
        }
        let target = url
            .query_pairs()
            .find(|(k, _)| k == "target")
            .map(|(_, v)| v.into_owned());
        let Some(target) = target else {
            return Response::error("symlink requires ?target=<path>", 400);
        };
        fs.symlink(&parsed.namespace, &target, &parsed.path).await?;
        return Response::from_json(&serde_json::json!({ "ok": true }));
    }

    if let Some(ns) = parse_ns("/glob", path) {
        let pattern = url
            .query_pairs()
            .find(|(k, _)| k == "pattern")
            .map(|(_, v)| v.into_owned());
        let Some(pattern) = pattern else {
            return Response::error("glob requires ?pattern=<glob>", 400);
        };
        let paths = fs.glob(&ns, &pattern).await?;
        return Response::from_json(&serde_json::json!({ "paths": paths }));
    }

    if let Some(ns) = parse_ns("/info", path) {
        let info = fs.workspace_info(&ns).await?;
        return Response::from_json(&serde_json::json!({ "info": info }));
    }

    Response::error("not found", 404)
}

struct Parsed {
    namespace: String,
    path: String,
}

/// `parse` variant for routes that take a namespace but no FS path
/// (today: `/glob/<ns>`, `/info/<ns>`). Accepts the prefix with or
/// without a trailing slash. Returns the namespace.
fn parse_ns(prefix: &str, path: &str) -> Option<String> {
    let stripped = path.strip_prefix(prefix)?;
    let ns = stripped.trim_start_matches('/').trim_end_matches('/');
    if ns.is_empty() {
        return None;
    }
    Some(ns.to_string())
}

fn parse(prefix: &str, path: &str) -> Option<Parsed> {
    let stripped = path.strip_prefix(prefix)?;
    let slash = stripped.find('/')?;
    let namespace = &stripped[..slash];
    let fs_path_raw = &stripped[slash..];
    if namespace.is_empty() || fs_path_raw.is_empty() {
        return None;
    }
    // `worker::Request::url()` doesn't decode the path; a URL like
    //   /fs/alice/notes%2Fdraft.md
    // arrives as `path = "/fs/alice/notes%2Fdraft.md"`. Decode after
    // splitting on the first literal "/" so callers can reference
    // paths with embedded slashes (or any URI-reserved char) by
    // percent-encoding them.
    let fs_path = percent_encoding::percent_decode_str(fs_path_raw)
        .decode_utf8()
        .ok()?
        .into_owned();
    Some(Parsed {
        namespace: namespace.to_string(),
        path: fs_path,
    })
}

const BANNER: &str = "\
cloudflare-shell-rpc-demo-rust

Routes:
  PUT    /fs/:ns/:path             -- write_file (raw body bytes)
  GET    /fs/:ns/:path             -- read_file (raw bytes back)
  DELETE /fs/:ns/:path             -- rm (?recursive=1&force=1)
  POST   /append/:ns/:path         -- append_file (raw body bytes)
  POST   /delete_file/:ns/:path    -- delete_file (file/symlink only)
  GET    /stat/:ns/:path           -- stat (follows symlinks)
  GET    /lstat/:ns/:path          -- lstat (no symlink follow)
  GET    /exists/:ns/:path         -- exists
  GET    /file_exists/:ns/:path    -- file_exists (symlink-resolving, file-only)
  GET    /list/:ns/:path           -- list (read_dir)
  POST   /mkdir/:ns/:path          -- mkdir (?recursive=1)
  POST   /cp/:ns/:src?dst&recursive -- cp
  POST   /mv/:ns/:src?dst          -- mv
  POST   /symlink/:ns/:link?target -- symlink
  GET    /readlink/:ns/:path       -- readlink
  GET    /realpath/:ns/:path       -- realpath
  GET    /glob/:ns?pattern         -- glob
  GET    /info/:ns                 -- workspace_info
";
