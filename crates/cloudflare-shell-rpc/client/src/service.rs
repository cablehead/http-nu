//! Typed `ShellFs` async trait + `ShellFsService` implementation.
//!
//! Each method shape:
//!   1. Build the matching wire request struct.
//!   2. `serde_wasm_bindgen::to_value` -> `JsValue`.
//!   3. Call the `sys::ShellFsSys` method, get a `Promise`.
//!   4. Await the promise via `JsFuture`.
//!   5. `serde_wasm_bindgen::from_value` -> typed response struct.
//!   6. Decode (e.g. base64 bytes) and return.
//!
//! Errors raised JS-side (thrown by the RPC server) surface as
//! `JsValue`; we convert to `worker::Error::RustError` carrying the
//! POSIX-prefixed message ("ENOENT: ...", "EISDIR: ...", etc.). Callers
//! can match on the prefix the same way they would after a
//! `cloudflare_shell::FsError` Display.

use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use cloudflare_shell_rpc_types::{
    Ack, AppendFileReq, CpReq, DeleteFileReq, DeleteFileResp, DirEntry, ExistsReq, ExistsResp,
    FileExistsReq, FileExistsResp, GlobReq, GlobResp, ListReq, ListResp, LstatReq, LstatResp,
    MkdirReq, MvReq, ReadFileReq, ReadFileResp, ReadlinkReq, ReadlinkResp, RealpathReq,
    RealpathResp, RmReq, Stat, StatReq, StatResp, SymlinkReq, WorkspaceInfo, WorkspaceInfoReq,
    WorkspaceInfoResp, WriteFileReq,
};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
use worker::send::{SendFuture, SendWrapper};
use worker::{Error, Fetcher, Result};

use crate::sys::ShellFsSys;

/// Typed surface for calling the `cloudflare-shell-rpc` Worker over a
/// service binding. The `Option<T>` returns reflect ENOENT semantics
/// (`Ok(None)` = path doesn't exist, not an error).
#[async_trait(?Send)]
pub trait ShellFs {
    async fn read_file(&self, namespace: &str, path: &str) -> Result<Option<Vec<u8>>>;
    async fn write_file(
        &self,
        namespace: &str,
        path: &str,
        data: &[u8],
        mime_type: Option<&str>,
    ) -> Result<()>;
    async fn stat(&self, namespace: &str, path: &str) -> Result<Option<Stat>>;
    async fn mkdir(&self, namespace: &str, path: &str, recursive: bool) -> Result<()>;
    async fn rm(&self, namespace: &str, path: &str, recursive: bool, force: bool) -> Result<()>;
    async fn list(&self, namespace: &str, path: &str) -> Result<Option<Vec<DirEntry>>>;
    async fn exists(&self, namespace: &str, path: &str) -> Result<bool>;
    async fn lstat(&self, namespace: &str, path: &str) -> Result<Option<Stat>>;
    async fn append_file(&self, namespace: &str, path: &str, data: &[u8]) -> Result<()>;
    async fn cp(&self, namespace: &str, src: &str, dst: &str, recursive: bool) -> Result<()>;
    async fn mv(&self, namespace: &str, src: &str, dst: &str) -> Result<()>;
    async fn symlink(&self, namespace: &str, target: &str, link_path: &str) -> Result<()>;
    async fn readlink(&self, namespace: &str, path: &str) -> Result<Option<String>>;
    async fn realpath(&self, namespace: &str, path: &str) -> Result<Option<String>>;
    async fn glob(&self, namespace: &str, pattern: &str) -> Result<Vec<String>>;
    async fn file_exists(&self, namespace: &str, path: &str) -> Result<bool>;
    async fn delete_file(&self, namespace: &str, path: &str) -> Result<bool>;
    async fn workspace_info(&self, namespace: &str) -> Result<WorkspaceInfo>;
}

/// Service-binding client. Obtain via `env.service("SHELL_FS")?.into()`
/// for a token-less (dev) server, or via [`ShellFsService::with_auth`]
/// to attach a token that matches the server's `SHELL_FS_TOKEN` env.
pub struct ShellFsService {
    sys: SendWrapper<ShellFsSys>,
    auth: Option<String>,
}

impl From<Fetcher> for ShellFsService {
    fn from(fetcher: Fetcher) -> Self {
        // `into_rpc()` returns the JS-side RPC handle for the bound
        // WorkerEntrypoint. SendWrapper makes the !Send JS value cross
        // .await boundaries in axum / tokio-on-wasm contexts.
        let raw: JsValue = fetcher.into_rpc();
        Self {
            sys: SendWrapper::new(raw.unchecked_into::<ShellFsSys>()),
            auth: None,
        }
    }
}

impl ShellFsService {
    /// Attach an auth token to every RPC the client issues. The token
    /// must match the server's `SHELL_FS_TOKEN` env var. If the server
    /// has no token set, this is a no-op.
    pub fn with_auth(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(token.into());
        self
    }
}

impl ShellFsService {
    async fn invoke<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
        &self,
        req: &Req,
        call: impl FnOnce(&ShellFsSys, JsValue) -> std::result::Result<js_sys::Promise, JsValue>,
    ) -> Result<Resp> {
        let args = serde_wasm_bindgen::to_value(req)
            .map_err(|e| Error::RustError(format!("encode args: {e}")))?;
        let promise = call(&self.sys, args).map_err(js_err)?;
        let value = SendFuture::new(JsFuture::from(promise))
            .await
            .map_err(js_err)?;
        serde_wasm_bindgen::from_value::<Resp>(value)
            .map_err(|e| Error::RustError(format!("decode response: {e}")))
    }
}

fn js_err(v: JsValue) -> Error {
    // JS-side thrown Error: stringify the message; the server's error
    // path emits POSIX-prefixed strings already.
    Error::RustError(
        v.as_string()
            .or_else(|| {
                // Plain Error objects -> .message
                let obj = v.dyn_ref::<js_sys::Object>()?;
                js_sys::Reflect::get(obj, &"message".into())
                    .ok()
                    .and_then(|m| m.as_string())
            })
            .unwrap_or_else(|| "rpc call threw non-string".to_string()),
    )
}

use wasm_bindgen::JsCast;

#[async_trait(?Send)]
impl ShellFs for ShellFsService {
    async fn read_file(&self, namespace: &str, path: &str) -> Result<Option<Vec<u8>>> {
        let req = ReadFileReq {
            namespace: namespace.into(),
            path: path.into(),
            auth: self.auth.clone(),
        };
        let resp: ReadFileResp = self.invoke(&req, |s, a| s.read_file(a)).await?;
        match resp.data {
            None => Ok(None),
            Some(b64) => {
                Ok(Some(B64.decode(b64.as_bytes()).map_err(|e| {
                    Error::RustError(format!("base64 decode: {e}"))
                })?))
            }
        }
    }

    async fn write_file(
        &self,
        namespace: &str,
        path: &str,
        data: &[u8],
        mime_type: Option<&str>,
    ) -> Result<()> {
        let req = WriteFileReq {
            namespace: namespace.into(),
            path: path.into(),
            data: B64.encode(data),
            mime_type: mime_type.map(str::to_owned),
            auth: self.auth.clone(),
        };
        let _ack: cloudflare_shell_rpc_types::Ack =
            self.invoke(&req, |s, a| s.write_file(a)).await?;
        Ok(())
    }

    async fn stat(&self, namespace: &str, path: &str) -> Result<Option<Stat>> {
        let req = StatReq {
            namespace: namespace.into(),
            path: path.into(),
            auth: self.auth.clone(),
        };
        let resp: StatResp = self.invoke(&req, |s, a| s.stat(a)).await?;
        Ok(resp.stat)
    }

    async fn mkdir(&self, namespace: &str, path: &str, recursive: bool) -> Result<()> {
        let req = MkdirReq {
            namespace: namespace.into(),
            path: path.into(),
            recursive,
            auth: self.auth.clone(),
        };
        let _ack: cloudflare_shell_rpc_types::Ack = self.invoke(&req, |s, a| s.mkdir(a)).await?;
        Ok(())
    }

    async fn rm(&self, namespace: &str, path: &str, recursive: bool, force: bool) -> Result<()> {
        let req = RmReq {
            namespace: namespace.into(),
            path: path.into(),
            recursive,
            force,
            auth: self.auth.clone(),
        };
        let _ack: cloudflare_shell_rpc_types::Ack = self.invoke(&req, |s, a| s.rm(a)).await?;
        Ok(())
    }

    async fn list(&self, namespace: &str, path: &str) -> Result<Option<Vec<DirEntry>>> {
        let req = ListReq {
            namespace: namespace.into(),
            path: path.into(),
            auth: self.auth.clone(),
        };
        let resp: ListResp = self.invoke(&req, |s, a| s.list(a)).await?;
        Ok(resp.entries)
    }

    async fn exists(&self, namespace: &str, path: &str) -> Result<bool> {
        let req = ExistsReq {
            namespace: namespace.into(),
            path: path.into(),
            auth: self.auth.clone(),
        };
        let resp: ExistsResp = self.invoke(&req, |s, a| s.exists(a)).await?;
        Ok(resp.exists)
    }

    async fn lstat(&self, namespace: &str, path: &str) -> Result<Option<Stat>> {
        let req = LstatReq {
            namespace: namespace.into(),
            path: path.into(),
            auth: self.auth.clone(),
        };
        let resp: LstatResp = self.invoke(&req, |s, a| s.lstat(a)).await?;
        Ok(resp.stat)
    }

    async fn append_file(&self, namespace: &str, path: &str, data: &[u8]) -> Result<()> {
        let req = AppendFileReq {
            namespace: namespace.into(),
            path: path.into(),
            data: B64.encode(data),
            auth: self.auth.clone(),
        };
        let _ack: Ack = self.invoke(&req, |s, a| s.append_file(a)).await?;
        Ok(())
    }

    async fn cp(&self, namespace: &str, src: &str, dst: &str, recursive: bool) -> Result<()> {
        let req = CpReq {
            namespace: namespace.into(),
            src: src.into(),
            dst: dst.into(),
            recursive,
            auth: self.auth.clone(),
        };
        let _ack: Ack = self.invoke(&req, |s, a| s.cp(a)).await?;
        Ok(())
    }

    async fn mv(&self, namespace: &str, src: &str, dst: &str) -> Result<()> {
        let req = MvReq {
            namespace: namespace.into(),
            src: src.into(),
            dst: dst.into(),
            auth: self.auth.clone(),
        };
        let _ack: Ack = self.invoke(&req, |s, a| s.mv(a)).await?;
        Ok(())
    }

    async fn symlink(&self, namespace: &str, target: &str, link_path: &str) -> Result<()> {
        let req = SymlinkReq {
            namespace: namespace.into(),
            target: target.into(),
            link_path: link_path.into(),
            auth: self.auth.clone(),
        };
        let _ack: Ack = self.invoke(&req, |s, a| s.symlink(a)).await?;
        Ok(())
    }

    async fn readlink(&self, namespace: &str, path: &str) -> Result<Option<String>> {
        let req = ReadlinkReq {
            namespace: namespace.into(),
            path: path.into(),
            auth: self.auth.clone(),
        };
        let resp: ReadlinkResp = self.invoke(&req, |s, a| s.readlink(a)).await?;
        Ok(resp.target)
    }

    async fn realpath(&self, namespace: &str, path: &str) -> Result<Option<String>> {
        let req = RealpathReq {
            namespace: namespace.into(),
            path: path.into(),
            auth: self.auth.clone(),
        };
        let resp: RealpathResp = self.invoke(&req, |s, a| s.realpath(a)).await?;
        Ok(resp.path)
    }

    async fn glob(&self, namespace: &str, pattern: &str) -> Result<Vec<String>> {
        let req = GlobReq {
            namespace: namespace.into(),
            pattern: pattern.into(),
            auth: self.auth.clone(),
        };
        let resp: GlobResp = self.invoke(&req, |s, a| s.glob(a)).await?;
        Ok(resp.paths)
    }

    async fn file_exists(&self, namespace: &str, path: &str) -> Result<bool> {
        let req = FileExistsReq {
            namespace: namespace.into(),
            path: path.into(),
            auth: self.auth.clone(),
        };
        let resp: FileExistsResp = self.invoke(&req, |s, a| s.file_exists(a)).await?;
        Ok(resp.exists)
    }

    async fn delete_file(&self, namespace: &str, path: &str) -> Result<bool> {
        let req = DeleteFileReq {
            namespace: namespace.into(),
            path: path.into(),
            auth: self.auth.clone(),
        };
        let resp: DeleteFileResp = self.invoke(&req, |s, a| s.delete_file(a)).await?;
        Ok(resp.removed)
    }

    async fn workspace_info(&self, namespace: &str) -> Result<WorkspaceInfo> {
        let req = WorkspaceInfoReq {
            namespace: namespace.into(),
            auth: self.auth.clone(),
        };
        let resp: WorkspaceInfoResp = self.invoke(&req, |s, a| s.workspace_info(a)).await?;
        Ok(resp.info)
    }
}
