//! The `FileSystem` trait + shared types, ported from
//! `.src/agents/packages/shell/src/fs/interface.ts` plus the type
//! exports at the top of `.src/agents/packages/shell/src/filesystem.ts`.
//!
//! The reference impl lives in the
//! [`cloudflare-shell-workspace`](https://docs.rs/cloudflare-shell-workspace)
//! crate (`Workspace`, backed by DO SQLite + R2). The conformance
//! suite in [`crate::conformance`] is generic over `<F: FileSystem>`
//! so any custom impl can run the same assertions.

use crate::error::Result;

/// Upstream: filesystem.ts:120 `EntryType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
}

impl EntryType {
    /// SQL `type` column value for this entry kind. Matches upstream's
    /// inline string literals at filesystem.ts:340-345.
    pub fn as_str(self) -> &'static str {
        match self {
            EntryType::File => "file",
            EntryType::Directory => "directory",
            EntryType::Symlink => "symlink",
        }
    }

    /// Inverse of [`as_str`](Self::as_str). Returns `None` for unknown
    /// strings (matches upstream's `parseEntryType` which throws).
    pub fn parse(s: &str) -> Option<EntryType> {
        match s {
            "file" => Some(EntryType::File),
            "directory" => Some(EntryType::Directory),
            "symlink" => Some(EntryType::Symlink),
            _ => None,
        }
    }
}

/// Upstream: filesystem.ts:122 `FileInfo` / `FileStat`.
#[derive(Debug, Clone)]
pub struct Stat {
    pub kind: EntryType,
    pub size: u64,
    pub modified_at: i64,
    pub mime_type: String,
    /// POSIX-style mode bits. Computed from `kind` at read time -- not
    /// stored. Matches upstream `@cloudflare/shell`'s behavior of
    /// computing modes at the FileSystem interface boundary.
    pub mode: u32,
}

impl Stat {
    pub fn mode_for(kind: EntryType) -> u32 {
        match kind {
            EntryType::File => DEFAULT_FILE_MODE,
            EntryType::Directory => DEFAULT_DIR_MODE,
            EntryType::Symlink => SYMLINK_MODE,
        }
    }
}

/// Upstream: fs/interface.ts:23 `FileSystemDirent`.
#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryType,
}

/// Upstream: fs/interface.ts:28 `MkdirOptions`.
#[derive(Debug, Clone, Default)]
pub struct MkdirOptions {
    pub recursive: bool,
}

/// Upstream: fs/interface.ts:32 `RmOptions`.
#[derive(Debug, Clone, Default)]
pub struct RmOptions {
    pub recursive: bool,
    pub force: bool,
}

/// Upstream: fs/interface.ts:37 `CpOptions`.
#[derive(Debug, Clone, Default)]
pub struct CpOptions {
    pub recursive: bool,
}

/// Upstream: filesystem.ts:135 `WorkspaceChangeType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceChangeType {
    Create,
    Update,
    Delete,
}

/// Upstream: filesystem.ts:137 `WorkspaceChangeEvent`.
#[derive(Debug, Clone)]
pub struct WorkspaceChangeEvent {
    pub kind: WorkspaceChangeType,
    pub path: String,
    pub entry_type: EntryType,
}

/// Boxed `onChange` listener. `Arc` so it can be cheaply cloned across
/// per-request `FileSystem` instances.
pub type OnChange = std::sync::Arc<dyn Fn(WorkspaceChangeEvent) + Send + Sync>;

// ── Constants (mirrors of upstream) ──────────────────────────────────

/// Upstream: filesystem.ts:193 `MAX_PATH_LENGTH = 4096`.
pub const MAX_PATH_LENGTH: usize = 4096;

/// Upstream: filesystem.ts (`MAX_SYMLINK_DEPTH = 40`, used inside
/// `resolveSymlink`).
pub const MAX_SYMLINK_DEPTH: u32 = 40;

/// Upstream: fs/path-utils.ts:8-10. POSIX-style mode bits surfaced on
/// `Stat`, computed at read time from `EntryType`.
pub const DEFAULT_FILE_MODE: u32 = 0o644;
pub const DEFAULT_DIR_MODE: u32 = 0o755;
pub const SYMLINK_MODE: u32 = 0o777;

/// Upstream: filesystem.ts:732 / L614 -- positional defaults for the
/// `mimeType` parameter on `writeFile` / `writeFileBytes`.
pub const DEFAULT_TEXT_MIME: &str = "text/plain";
pub const DEFAULT_BYTES_MIME: &str = "application/octet-stream";

/// Upstream: filesystem.ts:191 `const MAX_STREAM_SIZE = 100 * 1024 * 1024`.
/// Maximum stream size accepted by `Workspace::write_file_stream` in the
/// default (faithful) mode. The faithful path buffers every chunk in
/// memory before delegating to `write_file_bytes`, so this cap is what
/// prevents a runaway producer from OOM-ing the isolate. Practical Workers
/// memory ceiling (128 MB) means real-world callers will hit allocation
/// pressure well before reaching this constant.
pub const MAX_STREAM_SIZE: usize = 100 * 1024 * 1024;

// ── The trait ────────────────────────────────────────────────────────

/// Upstream: fs/interface.ts:52 `interface FileSystem`.
///
/// Deviations from the TS shape:
///   - **`Ok(None)` instead of throwing on ENOENT.** Read-side methods
///     (`stat`, `lstat`, `read_file*`, `readlink`, `read_dir*`,
///     `realpath`) return `Ok(None)` when the path doesn't exist;
///     EISDIR / ENOTDIR / etc. still return `Err(...)`. Upstream throws
///     in all cases. Documented in this crate's `CLAUDE.md`.
///   - **`mime_type: Option<&str>` on writes.** Matches the upstream
///     positional `mimeType` default ("text/plain" / "application/octet-stream"
///     when `None`).
///   - **`async fn` directly in the trait** (Rust 1.75+). Not
///     `dyn`-compatible by default; use `<F: FileSystem>` generics.
pub trait FileSystem {
    /// Upstream: `exists()` -- filesystem.ts:1028 / in-memory-fs.ts:286.
    fn exists(&self, path: &str) -> impl std::future::Future<Output = Result<bool>>;

    /// Upstream: `stat()` -- filesystem.ts:500 / in-memory-fs.ts:296.
    /// Follows symlinks. `Ok(None)` on ENOENT.
    fn stat(&self, path: &str) -> impl std::future::Future<Output = Result<Option<Stat>>>;

    /// Upstream: `lstat()` -- filesystem.ts:475 / in-memory-fs.ts:319.
    /// Does NOT follow symlinks.
    fn lstat(&self, path: &str) -> impl std::future::Future<Output = Result<Option<Stat>>>;

    /// Upstream: `readFile()` -- filesystem.ts:526 / in-memory-fs.ts:212.
    fn read_file(&self, path: &str) -> impl std::future::Future<Output = Result<Option<String>>>;

    /// Upstream: `readFileBytes()` -- filesystem.ts:569 / in-memory-fs.ts:219.
    /// `EISDIR` on dir; `Ok(None)` on ENOENT.
    fn read_file_bytes(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = Result<Option<Vec<u8>>>>;

    /// Upstream: `writeFile(path, content, mimeType = "text/plain")`.
    fn write_file(
        &self,
        path: &str,
        content: &str,
        mime_type: Option<&str>,
    ) -> impl std::future::Future<Output = Result<()>>;

    /// Upstream: `writeFileBytes(path, data, mimeType = "application/octet-stream")`.
    fn write_file_bytes(
        &self,
        path: &str,
        content: &[u8],
        mime_type: Option<&str>,
    ) -> impl std::future::Future<Output = Result<()>>;

    /// Upstream: `appendFile()`. Preserves existing entry's `mime_type`.
    fn append_file(
        &self,
        path: &str,
        content: &[u8],
    ) -> impl std::future::Future<Output = Result<()>>;

    /// Upstream: `readdir()`.
    fn read_dir(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = Result<Option<Vec<String>>>>;

    /// Upstream: `readdirWithFileTypes()`.
    fn read_dir_with_file_types(
        &self,
        path: &str,
    ) -> impl std::future::Future<Output = Result<Option<Vec<DirEntry>>>>;

    /// Upstream: `mkdir(path, { recursive })`.
    fn mkdir(
        &self,
        path: &str,
        opts: MkdirOptions,
    ) -> impl std::future::Future<Output = Result<()>>;

    /// Upstream: `rm(path, { recursive, force })`.
    fn rm(&self, path: &str, opts: RmOptions) -> impl std::future::Future<Output = Result<()>>;

    /// Upstream: `cp(src, dst, { recursive })`. Preserves source `mime_type`.
    fn cp(
        &self,
        src: &str,
        dst: &str,
        opts: CpOptions,
    ) -> impl std::future::Future<Output = Result<()>>;

    /// Upstream: `mv(src, dst)`.
    fn mv(&self, src: &str, dst: &str) -> impl std::future::Future<Output = Result<()>>;

    /// Upstream: `symlink(target, linkPath)`. Always emits `Create`.
    fn symlink(
        &self,
        target: &str,
        link_path: &str,
    ) -> impl std::future::Future<Output = Result<()>>;

    /// Upstream: `readlink()`. `Ok(None)` if path isn't a symlink.
    fn readlink(&self, path: &str) -> impl std::future::Future<Output = Result<Option<String>>>;

    /// Upstream: `realpath()`. Resolves symlinks.
    fn realpath(&self, path: &str) -> impl std::future::Future<Output = Result<Option<String>>>;

    /// Upstream: `glob()`. Returns absolute paths, sorted.
    fn glob(&self, pattern: &str) -> impl std::future::Future<Output = Result<Vec<String>>>;

    /// Upstream: `resolvePath(base, path)` -- fs/interface.ts:72.
    /// Pure path math; absolute `path` is normalized as-is, relative
    /// `path` is joined onto `base` first. Synchronous: no I/O, no DB
    /// access, no symlink resolution. Default impl delegates to
    /// [`crate::path_utils::resolve_path`].
    fn resolve_path(&self, base: &str, path: &str) -> String {
        crate::path_utils::resolve_path(base, path)
    }
}
