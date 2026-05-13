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

    Response::error("not found", 404)
}

struct Parsed {
    namespace: String,
    path: String,
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
  PUT    /fs/:ns/:path           -- write_file (raw body bytes)
  GET    /fs/:ns/:path           -- read_file (raw bytes back)
  DELETE /fs/:ns/:path           -- rm (?recursive=1&force=1)
  GET    /stat/:ns/:path         -- stat
  GET    /list/:ns/:path         -- list
  POST   /mkdir/:ns/:path        -- mkdir (?recursive=1)
";
