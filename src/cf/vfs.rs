//! Vfs trait + SnapshotVfs impl for the CF target.
//!
//! Lives entirely under `src/cf/` (never touches upstream files) so a
//! cablehead/http-nu merge doesn't conflict with our additions. If
//! desktop ever wants the same shadow-command surface (route Nu fs ops
//! through a Workspace-style trait), the trait promotes to a top-level
//! `src/vfs.rs` then -- not now.
//!
//! Nu commands run synchronously; @cloudflare/shell's Workspace ops are
//! async (R2 spillover etc.). The CF handler preloads a snapshot from
//! Workspace before invoking the closure -- async preload happens here,
//! sync reads happen inside the Nu eval.
//!
//! Storage shape: `Rc<RefCell<SnapshotInner>>`. The CF handler creates
//! a `SnapshotVfs` (cheap Rc clone) and installs a boxed clone via
//! `install_vfs` for Nu commands to see. Both handles reference the
//! same `SnapshotInner`, so pending writes queued from inside Nu show
//! up when the handler drains afterward.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::shell::Workspace;
use crate::shell::EntryType;

// ── trait + types ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatKind {
    File,
    Dir,
    Symlink,
}

#[derive(Debug, Clone)]
pub struct Stat {
    pub kind: StatKind,
    pub size: u64,
}

pub trait Vfs {
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn stat(&self, path: &Path) -> io::Result<Stat>;
    fn mkdir(&self, path: &Path) -> io::Result<()>;
    fn rm(&self, path: &Path) -> io::Result<()>;
    fn for_each_path(&self, f: &mut dyn FnMut(&str));
}

thread_local! {
    /// The active Vfs for the current request. Per-DO-isolate. The CF
    /// handler installs a SnapshotVfs before invoking the closure and
    /// clears it after.
    pub static VFS_HANDLE: RefCell<Option<Box<dyn Vfs>>> = const { RefCell::new(None) };
}

pub fn install_vfs(v: Box<dyn Vfs>) {
    VFS_HANDLE.with(|cell| *cell.borrow_mut() = Some(v));
}

pub fn drop_vfs() {
    VFS_HANDLE.with(|cell| *cell.borrow_mut() = None);
}

pub fn with_vfs<F, R>(f: F) -> R
where
    F: FnOnce(Option<&dyn Vfs>) -> R,
{
    VFS_HANDLE.with(|cell| f(cell.borrow().as_deref()))
}

// ── SnapshotVfs (current impl, unchanged below) ──────────────────────

#[derive(Default, Debug)]
struct SnapshotInner {
    files: HashMap<PathBuf, Vec<u8>>,
    dirs: HashMap<PathBuf, Vec<PathBuf>>,
    stats: HashMap<PathBuf, Stat>,
    pending_writes: HashMap<PathBuf, Vec<u8>>,
    pending_ops: Vec<PendingOp>,
}

#[derive(Debug, Clone)]
pub enum PendingOp {
    Mkdir(PathBuf),
    Rm(PathBuf),
}

#[derive(Debug, Clone, Default)]
pub struct SnapshotVfs {
    inner: Rc<RefCell<SnapshotInner>>,
}

impl SnapshotVfs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Walk a Workspace tree to depth `max_depth` and capture files +
    /// stats. Files larger than `inline_limit` are stat-only (their
    /// content is left out so the snapshot stays small; a read for them
    /// returns ENOENT).
    pub async fn load_from_workspace(
        ws: &Workspace,
        max_depth: u32,
        inline_limit: u64,
    ) -> worker::Result<Self> {
        let snap = SnapshotVfs::new();
        Self::walk(ws, "/", 0, max_depth, inline_limit, &snap).await?;
        Ok(snap)
    }

    fn walk<'a>(
        ws: &'a Workspace,
        path: &'a str,
        depth: u32,
        max_depth: u32,
        inline_limit: u64,
        snap: &'a SnapshotVfs,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = worker::Result<()>> + 'a>> {
        Box::pin(async move {
            let entries = match ws.read_dir_with_file_types(path).await? {
                Some(e) => e,
                None => return Ok(()),
            };
            let mut child_paths = Vec::with_capacity(entries.len());
            for e in entries {
                let child = if path == "/" {
                    format!("/{}", e.name)
                } else {
                    format!("{path}/{}", e.name)
                };
                child_paths.push(PathBuf::from(&child));
                match e.kind {
                    EntryType::File => {
                        if let Some(stat) = ws.lstat(&child).await? {
                            let mut inner = snap.inner.borrow_mut();
                            inner.stats.insert(
                                PathBuf::from(&child),
                                Stat {
                                    kind: StatKind::File,
                                    size: stat.size,
                                },
                            );
                            if stat.size <= inline_limit {
                                drop(inner);
                                if let Some(bytes) = ws.read_file_bytes(&child).await? {
                                    snap.inner
                                        .borrow_mut()
                                        .files
                                        .insert(PathBuf::from(&child), bytes);
                                }
                            }
                        }
                    }
                    EntryType::Directory => {
                        snap.inner.borrow_mut().stats.insert(
                            PathBuf::from(&child),
                            Stat {
                                kind: StatKind::Dir,
                                size: 0,
                            },
                        );
                        if depth + 1 < max_depth {
                            Self::walk(ws, &child, depth + 1, max_depth, inline_limit, snap)
                                .await?;
                        }
                    }
                    EntryType::Symlink => {
                        snap.inner.borrow_mut().stats.insert(
                            PathBuf::from(&child),
                            Stat {
                                kind: StatKind::Symlink,
                                size: 0,
                            },
                        );
                    }
                }
            }
            let mut inner = snap.inner.borrow_mut();
            inner.dirs.insert(PathBuf::from(path), child_paths);
            inner.stats.insert(
                PathBuf::from(path),
                Stat {
                    kind: StatKind::Dir,
                    size: 0,
                },
            );
            Ok(())
        })
    }

    pub fn drain_pending_writes(&self) -> Vec<(PathBuf, Vec<u8>)> {
        let mut inner = self.inner.borrow_mut();
        std::mem::take(&mut inner.pending_writes)
            .into_iter()
            .collect()
    }

    pub fn drain_pending_ops(&self) -> Vec<PendingOp> {
        let mut inner = self.inner.borrow_mut();
        std::mem::take(&mut inner.pending_ops)
    }
}

impl Vfs for SnapshotVfs {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        let bytes = self.read_bytes(path)?;
        String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        let inner = self.inner.borrow();
        if let Some(data) = inner.pending_writes.get(path) {
            return Ok(data.clone());
        }
        inner
            .files
            .get(path)
            .cloned()
            .ok_or_else(|| not_found(path))
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        self.inner
            .borrow_mut()
            .pending_writes
            .insert(path.to_path_buf(), data.to_vec());
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        let inner = self.inner.borrow();
        inner.pending_writes.contains_key(path)
            || inner.files.contains_key(path)
            || inner.dirs.contains_key(path)
            || inner.stats.contains_key(path)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let inner = self.inner.borrow();
        inner.dirs.get(path).cloned().ok_or_else(|| not_found(path))
    }

    fn stat(&self, path: &Path) -> io::Result<Stat> {
        let inner = self.inner.borrow();
        if let Some(data) = inner.pending_writes.get(path) {
            return Ok(Stat {
                kind: StatKind::File,
                size: data.len() as u64,
            });
        }
        inner
            .stats
            .get(path)
            .cloned()
            .ok_or_else(|| not_found(path))
    }

    fn mkdir(&self, path: &Path) -> io::Result<()> {
        let p = path.to_path_buf();
        let mut inner = self.inner.borrow_mut();
        inner.stats.insert(
            p.clone(),
            Stat {
                kind: StatKind::Dir,
                size: 0,
            },
        );
        inner.dirs.entry(p.clone()).or_default();
        inner.pending_ops.push(PendingOp::Mkdir(p));
        Ok(())
    }

    fn rm(&self, path: &Path) -> io::Result<()> {
        let p = path.to_path_buf();
        let mut inner = self.inner.borrow_mut();
        inner.files.remove(&p);
        inner.dirs.remove(&p);
        inner.stats.remove(&p);
        inner.pending_writes.remove(&p);
        inner.pending_ops.push(PendingOp::Rm(p));
        Ok(())
    }

    fn for_each_path(&self, f: &mut dyn FnMut(&str)) {
        let inner = self.inner.borrow();
        for p in inner.stats.keys() {
            if let Some(s) = p.to_str() {
                f(s);
            }
        }
        for p in inner.pending_writes.keys() {
            if !inner.stats.contains_key(p) {
                if let Some(s) = p.to_str() {
                    f(s);
                }
            }
        }
    }
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("{}", path.display()))
}
