//! `ShellFsDo` -- the DurableObject. One instance per namespace; each
//! holds its own `Workspace` (DO SQLite + R2 spill). Receives internal
//! fetches from the entrypoint and dispatches by path.
//!
//! Why a DO at all: `cloudflare_shell_workspace::Workspace` needs
//! `SqlStorage` (DO-local SQLite) + an optional R2 bucket. There's no
//! other place in Workers to host that.
//!
//! Why internal-fetch instead of typed DO methods: worker-rs 0.8
//! `#[durable_object]` only exposes the `fetch(req)` handler. Once
//! workers-rs grows typed DO RPC, this module simplifies to direct
//! method calls; the wire shape doesn't change.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use cloudflare_shell::{CpOptions, FsError, MkdirOptions, RmOptions};
use cloudflare_shell_rpc_types::{
    Ack, AppendFileReq, CpReq, DeleteFileReq, DeleteFileResp, DirEntry as WireDirEntry,
    EntryType as WireEntryType, ExistsReq, ExistsResp, FileExistsReq, FileExistsResp, GlobReq,
    GlobResp, ListReq, ListResp, LstatReq, LstatResp, MkdirReq, MvReq, ReadFileReq, ReadFileResp,
    ReadlinkReq, ReadlinkResp, RealpathReq, RealpathResp, RmReq, Stat as WireStat, StatReq,
    StatResp, SymlinkReq, WorkspaceInfo as WireWorkspaceInfo, WorkspaceInfoReq, WorkspaceInfoResp,
    WriteFileReq,
};
use cloudflare_shell_workspace::Workspace;
use worker::{
    durable_object, DurableObject, Env, Request as WorkerRequest, Response, Result, State,
};

use crate::wire::{err_response, fs_error_to_rpc, ok_response};

const R2_BINDING: &str = "SHELL_FS_FILES";

#[durable_object]
pub struct ShellFsDo {
    state: State,
    env: Env,
    // Cache of per-namespace Workspace handles. Constructing a
    // Workspace runs `bootstrap()` -- CREATE TABLE IF NOT EXISTS +
    // CREATE INDEX + count-root + maybe INSERT-root -- which is
    // idempotent but still 3-4 sync SQL execs. We want exactly one
    // per (DO instance, namespace) lifetime, not per RPC call.
    //
    // RefCell + Rc are fine here because Workers DOs are
    // single-threaded by construction (one isolate, one task at a
    // time). Send/Sync don't come up.
    workspaces: RefCell<HashMap<String, Rc<Workspace>>>,
}

impl DurableObject for ShellFsDo {
    fn new(state: State, env: Env) -> Self {
        Self {
            state,
            env,
            workspaces: RefCell::new(HashMap::new()),
        }
    }

    async fn fetch(&self, mut req: WorkerRequest) -> Result<Response> {
        console_error_panic_hook::set_once();
        let url = req.url()?;
        let path = url.path().to_string();
        match path.as_str() {
            "/read_file" => self.handle_read_file(&mut req).await,
            "/write_file" => self.handle_write_file(&mut req).await,
            "/stat" => self.handle_stat(&mut req).await,
            "/mkdir" => self.handle_mkdir(&mut req).await,
            "/rm" => self.handle_rm(&mut req).await,
            "/list" => self.handle_list(&mut req).await,
            "/exists" => self.handle_exists(&mut req).await,
            "/lstat" => self.handle_lstat(&mut req).await,
            "/append_file" => self.handle_append_file(&mut req).await,
            "/cp" => self.handle_cp(&mut req).await,
            "/mv" => self.handle_mv(&mut req).await,
            "/symlink" => self.handle_symlink(&mut req).await,
            "/readlink" => self.handle_readlink(&mut req).await,
            "/realpath" => self.handle_realpath(&mut req).await,
            "/glob" => self.handle_glob(&mut req).await,
            "/file_exists" => self.handle_file_exists(&mut req).await,
            "/delete_file" => self.handle_delete_file(&mut req).await,
            "/workspace_info" => self.handle_workspace_info(&mut req).await,
            other => Response::error(format!("unknown internal route: {other}"), 404),
        }
    }
}

/// Resolve the per-handler `Workspace` or short-circuit with a typed
/// `err_response`. We can't use `?` directly because the `FsError` (e.g.
/// the namespace-validation error from `Workspace::new`) needs to come
/// back to the caller as a typed `RpcError` on the wire, not as the
/// generic `worker::Error::RustError` that `?` would produce via the
/// `From<FsError>` impl.
macro_rules! open_ws {
    ($self:expr, $ns:expr) => {
        match $self.open_workspace($ns) {
            Ok(ws) => ws,
            Err(e) => return $crate::wire::err_response($crate::wire::fs_error_to_rpc(e)),
        }
    };
}

impl ShellFsDo {
    /// Returns a cached `Workspace` for the namespace, constructing one
    /// (and running its idempotent bootstrap) only on first hit per DO
    /// lifetime.
    ///
    /// Cache key is the request-supplied namespace string. Empty maps
    /// to the default-namespaced workspace (same as
    /// `id_from_name("")`'s implicit-default). Any other namespace
    /// goes through `Workspace::new`, which enforces upstream's
    /// VALID_NAMESPACE regex -- a bad namespace fails here once and
    /// is NOT cached (the error surfaces through `open_ws!`).
    fn open_workspace(&self, namespace: &str) -> std::result::Result<Rc<Workspace>, FsError> {
        if let Some(ws) = self.workspaces.borrow().get(namespace).cloned() {
            return Ok(ws);
        }
        let sql = self.state.storage().sql();
        let r2 = self.env.bucket(R2_BINDING).ok();
        let ws = if namespace.is_empty() {
            Workspace::default(sql, r2)?
        } else {
            Workspace::new(sql, r2, namespace)?
        };
        let rc = Rc::new(ws);
        self.workspaces
            .borrow_mut()
            .insert(namespace.to_string(), Rc::clone(&rc));
        Ok(rc)
    }

    async fn handle_read_file(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: ReadFileReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.read_file_bytes(&body.path).await {
            Ok(Some(bytes)) => ok_response(&ReadFileResp {
                data: Some(B64.encode(bytes)),
            }),
            Ok(None) => ok_response(&ReadFileResp { data: None }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_write_file(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: WriteFileReq = req.json().await?;
        let bytes = match B64.decode(body.data.as_bytes()) {
            Ok(b) => b,
            Err(e) => {
                return err_response(cloudflare_shell_rpc_types::RpcError::InvalidUtf8(format!(
                    "data is not valid base64: {e}"
                )))
            }
        };
        let ws = open_ws!(self, &body.namespace);
        match ws
            .write_file_bytes(&body.path, &bytes, body.mime_type.as_deref())
            .await
        {
            Ok(()) => ok_response(&Ack::default()),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_stat(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: StatReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.stat(&body.path).await {
            Ok(stat) => ok_response(&StatResp {
                stat: stat.map(stat_to_wire),
            }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_mkdir(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: MkdirReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws
            .mkdir(
                &body.path,
                MkdirOptions {
                    recursive: body.recursive,
                },
            )
            .await
        {
            Ok(()) => ok_response(&Ack::default()),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_rm(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: RmReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws
            .rm(
                &body.path,
                RmOptions {
                    recursive: body.recursive,
                    force: body.force,
                },
            )
            .await
        {
            Ok(()) => ok_response(&Ack::default()),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_list(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: ListReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.read_dir_with_file_types(&body.path).await {
            Ok(entries) => ok_response(&ListResp {
                entries: entries.map(|v| v.into_iter().map(dir_entry_to_wire).collect()),
            }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_exists(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: ExistsReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.exists(&body.path).await {
            Ok(exists) => ok_response(&ExistsResp { exists }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_lstat(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: LstatReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.lstat(&body.path).await {
            Ok(stat) => ok_response(&LstatResp {
                stat: stat.map(stat_to_wire),
            }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_append_file(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: AppendFileReq = req.json().await?;
        let bytes = match B64.decode(body.data.as_bytes()) {
            Ok(b) => b,
            Err(e) => {
                return err_response(cloudflare_shell_rpc_types::RpcError::InvalidUtf8(format!(
                    "data is not valid base64: {e}"
                )))
            }
        };
        let ws = open_ws!(self, &body.namespace);
        match ws.append_file(&body.path, &bytes).await {
            Ok(()) => ok_response(&Ack::default()),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_cp(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: CpReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws
            .cp(
                &body.src,
                &body.dst,
                CpOptions {
                    recursive: body.recursive,
                },
            )
            .await
        {
            Ok(()) => ok_response(&Ack::default()),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_mv(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: MvReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.mv(&body.src, &body.dst).await {
            Ok(()) => ok_response(&Ack::default()),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_symlink(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: SymlinkReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.symlink(&body.target, &body.link_path).await {
            Ok(()) => ok_response(&Ack::default()),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_readlink(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: ReadlinkReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.readlink(&body.path).await {
            Ok(target) => ok_response(&ReadlinkResp { target }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_realpath(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: RealpathReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.realpath(&body.path).await {
            Ok(path) => ok_response(&RealpathResp { path }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_glob(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: GlobReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.glob(&body.pattern).await {
            Ok(paths) => ok_response(&GlobResp { paths }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_file_exists(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: FileExistsReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.file_exists(&body.path).await {
            Ok(exists) => ok_response(&FileExistsResp { exists }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_delete_file(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: DeleteFileReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.delete_file(&body.path).await {
            Ok(removed) => ok_response(&DeleteFileResp { removed }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }

    async fn handle_workspace_info(&self, req: &mut WorkerRequest) -> Result<Response> {
        let body: WorkspaceInfoReq = req.json().await?;
        let ws = open_ws!(self, &body.namespace);
        match ws.get_workspace_info().await {
            Ok(info) => ok_response(&WorkspaceInfoResp {
                info: WireWorkspaceInfo {
                    file_count: info.file_count,
                    directory_count: info.directory_count,
                    total_bytes: info.total_bytes,
                    r2_file_count: info.r2_file_count,
                },
            }),
            Err(e) => err_response(fs_error_to_rpc(e)),
        }
    }
}

fn entry_type_to_wire(t: cloudflare_shell::EntryType) -> WireEntryType {
    match t {
        cloudflare_shell::EntryType::File => WireEntryType::File,
        cloudflare_shell::EntryType::Directory => WireEntryType::Directory,
        cloudflare_shell::EntryType::Symlink => WireEntryType::Symlink,
    }
}

fn stat_to_wire(s: cloudflare_shell::Stat) -> WireStat {
    WireStat {
        kind: entry_type_to_wire(s.kind),
        size: s.size,
        modified_at: s.modified_at,
        mime_type: s.mime_type,
        mode: s.mode,
    }
}

fn dir_entry_to_wire(e: cloudflare_shell::DirEntry) -> WireDirEntry {
    WireDirEntry {
        name: e.name,
        kind: entry_type_to_wire(e.kind),
    }
}
