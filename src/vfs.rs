//! Filesystem abstraction shared by desktop AND wasm/CF targets.
//!
//! The `Vfs` trait + per-thread install/with hooks let upstream files
//! (`src/commands.rs`, `src/response.rs`, ...) do filesystem ops without
//! caring whether they're running on the real OS or inside a CF
//! Workers DurableObject. Each target provides an impl:
//!
//! - `OsVfs` (this file, `#[cfg(feature = "desktop")]`) -- thin wrapper
//!   over `std::fs::*`.
//! - `crate::cf::snapshot_vfs::SnapshotVfs` (CF only) -- per-request preload of
//!   the user's Workspace, drained back after eval.
//!
//! On desktop, `with_vfs` returns `OsVfs` as a default when nothing is
//! installed in the thread-local. So upstream code can just write
//!
//! ```ignore
//! crate::vfs::with_vfs(|maybe_vfs| match maybe_vfs {
//!     Some(v) => v.read_to_string(path),
//!     None    => Err(io::Error::new(io::ErrorKind::Other, "no vfs")),
//! })
//! ```
//!
//! and get the right impl on either target -- no cfg gates at the call
//! site. On wasm, no Vfs installed = `None`; the CF handler always
//! installs `SnapshotVfs` before invoking the engine, so this only
//! happens during early engine init (caller decides what to do).

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};

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
    /// The active Vfs for the current request, if explicitly installed.
    /// CF's fetch handler installs a `SnapshotVfs` here per request;
    /// desktop usually leaves this `None` and lets `with_vfs` fall
    /// back to `OsVfs`.
    static VFS_HANDLE: RefCell<Option<Box<dyn Vfs>>> = const { RefCell::new(None) };
}

pub fn install_vfs(v: Box<dyn Vfs>) {
    VFS_HANDLE.with(|cell| *cell.borrow_mut() = Some(v));
}

/// Resolve `path` to absolute. If already absolute, returns as-is.
/// On desktop, falls back to `std::env::current_dir()`; on wasm, treats
/// relative paths as workspace-rooted (`/path`). Use this anywhere
/// upstream code wants "the absolute path, however that means on this
/// target" -- avoids per-call-site `cfg` gates.
pub fn resolve_relative(path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    #[cfg(feature = "desktop")]
    {
        std::env::current_dir().unwrap_or_default().join(path)
    }
    #[cfg(not(feature = "desktop"))]
    {
        PathBuf::from("/").join(path)
    }
}

pub fn drop_vfs() {
    VFS_HANDLE.with(|cell| *cell.borrow_mut() = None);
}

/// Run `f` with the current Vfs. On desktop, defaults to `OsVfs` when
/// nothing's installed. On wasm, returns `None` when nothing's
/// installed (caller decides whether that's an error).
pub fn with_vfs<F, R>(f: F) -> R
where
    F: FnOnce(Option<&dyn Vfs>) -> R,
{
    VFS_HANDLE.with(|cell| {
        let borrowed = cell.borrow();
        if let Some(boxed) = borrowed.as_deref() {
            return f(Some(boxed));
        }
        #[cfg(feature = "desktop")]
        {
            let default = OsVfs;
            f(Some(&default))
        }
        #[cfg(not(feature = "desktop"))]
        {
            f(None)
        }
    })
}

// ── OsVfs: desktop impl over std::fs ─────────────────────────────────

#[cfg(feature = "desktop")]
pub struct OsVfs;

#[cfg(feature = "desktop")]
impl Vfs for OsVfs {
    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }

    fn write(&self, path: &Path, data: &[u8]) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(path, data)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        std::fs::read_dir(path)?
            .map(|entry| entry.map(|e| e.path()))
            .collect()
    }

    fn stat(&self, path: &Path) -> io::Result<Stat> {
        let meta = std::fs::symlink_metadata(path)?;
        let kind = if meta.file_type().is_symlink() {
            StatKind::Symlink
        } else if meta.is_dir() {
            StatKind::Dir
        } else {
            StatKind::File
        };
        Ok(Stat {
            kind,
            size: meta.len(),
        })
    }

    fn mkdir(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn rm(&self, path: &Path) -> io::Result<()> {
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
            Ok(_) => std::fs::remove_file(path),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn for_each_path(&self, _f: &mut dyn FnMut(&str)) {
        // No-op on OS-backed FS: a generic recursive walk has no useful
        // bound on desktop. Callers that need this should walk the
        // explicit subtree they care about via `read_dir`.
    }
}
