//! Rust port of @cloudflare/shell's `Workspace` filesystem.
//!
//! Backs a per-user FS on a Durable Object using DO SQLite for the file
//! index + content (up to 1.5MB inline) and R2 for spillover. Schema is
//! byte-compatible with the JS package (same `cf_workspace_<ns>` table,
//! same columns, same CHECK constraints, same `${name}/${ns}<path>` R2
//! key shape) so data is interoperable both ways.
//!
//! Why a Rust port:
//!   - The JS package can't be wired in from workers-rs Rust without
//!     fighting wasm-bindgen + worker-build (see workers-rs#998).
//!   - Cablehead stack (http-nu, yoke, xs) is all-Rust. A Rust crate
//!     lands in the same toolchain every project already uses.
//!
//! API shape:
//!   - All methods are `async fn`. `SqlStorage::exec` is sync underneath
//!     so the read path costs no real `.await`, but the async signature
//!     lets R2 ops compose naturally and matches @cloudflare/shell's
//!     surface (which is all `Promise<T>`).
//!   - ENOENT semantics: methods that look up a path return `Ok(None)`
//!     when the path doesn't exist. Callers that need ENOENT-as-error
//!     wrap in their own adapter (e.g. crate::cf::vfs's SnapshotVfs).
//!
//! Lives at `src/cf/workspace/` for now; intended to extract to its own
//! crate (`cf-workspace`) once stable so yoke + xs + future Rust-on-CF
//! projects can depend on it directly.

mod paths;
mod schema;

pub use schema::DEFAULT_NAMESPACE;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use worker::{Bucket, Result, SqlStorage};

use paths::{normalize, parent_path, path_name};

const MAX_SYMLINK_DEPTH: u32 = 40;
const R2_SPILL_THRESHOLD: usize = 1_500_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    File,
    Directory,
    Symlink,
}

impl EntryType {
    fn as_str(self) -> &'static str {
        match self {
            EntryType::File => "file",
            EntryType::Directory => "directory",
            EntryType::Symlink => "symlink",
        }
    }

    fn parse(s: &str) -> Option<EntryType> {
        match s {
            "file" => Some(EntryType::File),
            "directory" => Some(EntryType::Directory),
            "symlink" => Some(EntryType::Symlink),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stat {
    pub kind: EntryType,
    pub size: u64,
    pub modified_at: i64,
    pub mime_type: String,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryType,
}

#[derive(Debug, Clone, Default)]
pub struct MkdirOptions {
    pub recursive: bool,
}

#[derive(Debug, Clone, Default)]
pub struct RmOptions {
    pub recursive: bool,
    pub force: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CpOptions {
    pub recursive: bool,
}

/// One Workspace = one user's filesystem. Cheap to construct;
/// `bootstrap()` is idempotent and only does work the first time.
#[derive(Debug, Clone)]
pub struct Workspace {
    sql: SqlStorage,
    r2: Option<Bucket>,
    table: String,
    index: String,
    namespace: String,
    /// Prefix for R2 keys. Defaults to `namespace` when r2 is set. Matches
    /// @cloudflare/shell's resolveR2Prefix(): final key is
    /// `${r2_prefix}/${namespace}<path>`.
    r2_prefix: String,
}

impl Workspace {
    pub fn new(sql: SqlStorage, r2: Option<Bucket>, namespace: &str) -> Result<Self> {
        let table = format!("cf_workspace_{namespace}");
        let index = format!("idx_{table}_parent_path");
        let r2_prefix = namespace.to_string();
        let ws = Self {
            sql,
            r2,
            table,
            index,
            namespace: namespace.to_string(),
            r2_prefix,
        };
        ws.bootstrap()?;
        Ok(ws)
    }

    pub fn default(sql: SqlStorage, r2: Option<Bucket>) -> Result<Self> {
        Self::new(sql, r2, DEFAULT_NAMESPACE)
    }

    fn bootstrap(&self) -> Result<()> {
        self.sql
            .exec(&schema::create_table_sql(&self.table), None)?;
        self.sql
            .exec(&schema::create_index_sql(&self.index, &self.table), None)?;
        let count = exec_count(
            &self.sql,
            &format!("SELECT count(*) AS c FROM {} WHERE path = '/'", self.table),
            None,
        )?;
        if count == 0 {
            self.sql.exec(
                &format!(
                    "INSERT INTO {} (path, parent_path, name, type, mime_type) \
                     VALUES ('/', '', '', 'directory', 'inode/directory')",
                    self.table
                ),
                None,
            )?;
        }
        Ok(())
    }

    pub async fn exists(&self, path: &str) -> Result<bool> {
        let p = normalize(path);
        let n = exec_count(
            &self.sql,
            &format!("SELECT 1 AS c FROM {} WHERE path = ? LIMIT 1", self.table),
            Some(vec![p.into()]),
        )?;
        Ok(n > 0)
    }

    /// stat follows symlinks. Returns Ok(None) on ENOENT.
    pub async fn stat(&self, path: &str) -> Result<Option<Stat>> {
        let resolved = self.resolve_symlinks(&normalize(path), 0).await?;
        match resolved {
            Some(p) => self.lstat(&p).await,
            None => Ok(None),
        }
    }

    /// lstat does NOT follow symlinks.
    pub async fn lstat(&self, path: &str) -> Result<Option<Stat>> {
        let p = normalize(path);
        let cursor = self.sql.exec(
            &format!(
                "SELECT type, size, modified_at, mime_type FROM {} WHERE path = ? LIMIT 1",
                self.table
            ),
            Some(vec![p.into()]),
        )?;
        let row = match cursor.next::<StatRow>().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e),
            None => return Ok(None),
        };
        let Some(kind) = EntryType::parse(&row.r#type) else {
            return Ok(None);
        };
        Ok(Some(Stat {
            kind,
            size: row.size as u64,
            modified_at: row.modified_at,
            mime_type: row.mime_type,
        }))
    }

    pub async fn read_file(&self, path: &str) -> Result<Option<String>> {
        let bytes = match self.read_file_bytes(path).await? {
            Some(b) => b,
            None => return Ok(None),
        };
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| worker::Error::RustError(format!("read_file: invalid utf8: {e}")))
    }

    pub async fn read_file_bytes(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let Some(resolved) = self.resolve_symlinks(&normalize(path), 0).await? else {
            return Ok(None);
        };
        let cursor = self.sql.exec(
            &format!(
                "SELECT storage_backend, content_encoding, content, r2_key \
                 FROM {} WHERE path = ? AND type = 'file' LIMIT 1",
                self.table
            ),
            Some(vec![resolved.clone().into()]),
        )?;
        let row = match cursor.next::<FileRow>().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e),
            None => return Ok(None),
        };
        let bytes = if row.storage_backend == "r2" {
            let Some(r2) = &self.r2 else {
                return Err(worker::Error::RustError(format!(
                    "read_file_bytes: {resolved} is R2-backed but no R2 bucket bound"
                )));
            };
            let Some(key) = row.r2_key else {
                return Err(worker::Error::RustError(format!(
                    "read_file_bytes: {resolved} storage_backend=r2 but r2_key is NULL"
                )));
            };
            let obj = match r2.get(&key).execute().await? {
                Some(o) => o,
                None => return Ok(None),
            };
            let body = obj.body().ok_or_else(|| {
                worker::Error::RustError(format!("read_file_bytes: R2 object {key} has no body"))
            })?;
            body.bytes().await?
        } else {
            // Inline. content_encoding='base64' for binary, anything else
            // treats `content` as utf8 text (matches @cloudflare/shell).
            let content = row.content.unwrap_or_default();
            if row.content_encoding == "base64" {
                B64.decode(content.as_bytes())
                    .map_err(|e| worker::Error::RustError(format!("base64 decode: {e}")))?
            } else {
                content.into_bytes()
            }
        };
        Ok(Some(bytes))
    }

    pub async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        self.write_inner(path, content.as_bytes(), "utf8").await
    }

    pub async fn write_file_bytes(&self, path: &str, content: &[u8]) -> Result<()> {
        // Choose encoding based on utf8-validity. utf8-clean bytes stay
        // as TEXT (cheaper, queryable); binary goes base64 (matches
        // @cloudflare/shell's content_encoding semantics).
        if let Ok(s) = std::str::from_utf8(content) {
            self.write_inner(path, s.as_bytes(), "utf8").await
        } else {
            self.write_inner(path, content, "base64").await
        }
    }

    pub async fn append_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let existing = self.read_file_bytes(path).await?.unwrap_or_default();
        let mut combined = Vec::with_capacity(existing.len() + content.len());
        combined.extend_from_slice(&existing);
        combined.extend_from_slice(content);
        self.write_file_bytes(path, &combined).await
    }

    async fn write_inner(&self, path: &str, content: &[u8], encoding: &str) -> Result<()> {
        let p = normalize(path);
        self.ensure_parent_dirs(&p)?;
        let parent = parent_path(&p);
        let name = path_name(&p);
        let size = content.len() as i64;

        if content.len() > R2_SPILL_THRESHOLD {
            let Some(r2) = &self.r2 else {
                return Err(worker::Error::RustError(format!(
                    "write: {p} would spill to R2 ({} bytes > {R2_SPILL_THRESHOLD}) but no R2 bucket bound",
                    content.len()
                )));
            };
            let key = self.r2_key(&p);
            r2.put(&key, content.to_vec()).execute().await?;
            self.sql.exec(
                &format!(
                    "INSERT INTO {table} \
                       (path, parent_path, name, type, size, storage_backend, r2_key, \
                        content_encoding, content, modified_at) \
                     VALUES (?, ?, ?, 'file', ?, 'r2', ?, ?, NULL, unixepoch()) \
                     ON CONFLICT(path) DO UPDATE SET \
                       size = excluded.size, \
                       storage_backend = 'r2', \
                       r2_key = excluded.r2_key, \
                       content_encoding = excluded.content_encoding, \
                       content = NULL, \
                       modified_at = unixepoch()",
                    table = self.table
                ),
                Some(vec![
                    p.clone().into(),
                    parent.into(),
                    name.into(),
                    size.into(),
                    key.into(),
                    encoding.into(),
                ]),
            )?;
            // If a previous inline version was there and we just spilled,
            // the UPDATE already cleared content; nothing else to do.
            return Ok(());
        }

        // Inline path.
        let content_str = match encoding {
            "base64" => B64.encode(content),
            _ => String::from_utf8_lossy(content).into_owned(),
        };
        self.sql.exec(
            &format!(
                "INSERT INTO {table} \
                   (path, parent_path, name, type, size, storage_backend, r2_key, \
                    content_encoding, content, modified_at) \
                 VALUES (?, ?, ?, 'file', ?, 'inline', NULL, ?, ?, unixepoch()) \
                 ON CONFLICT(path) DO UPDATE SET \
                   size = excluded.size, \
                   storage_backend = 'inline', \
                   r2_key = NULL, \
                   content_encoding = excluded.content_encoding, \
                   content = excluded.content, \
                   modified_at = unixepoch()",
                table = self.table
            ),
            Some(vec![
                p.clone().into(),
                parent.into(),
                name.into(),
                size.into(),
                encoding.into(),
                content_str.into(),
            ]),
        )?;
        // If we just shrank from R2 -> inline, delete the orphaned R2 key.
        // We can't tell from here without an extra SELECT, so we always
        // best-effort delete the key we'd have used. Idempotent.
        if let Some(r2) = &self.r2 {
            let key = self.r2_key(&p);
            let _ = r2.delete(&key).await;
        }
        Ok(())
    }

    pub async fn read_dir(&self, path: &str) -> Result<Option<Vec<String>>> {
        let Some(entries) = self.read_dir_with_file_types(path).await? else {
            return Ok(None);
        };
        Ok(Some(entries.into_iter().map(|e| e.name).collect()))
    }

    pub async fn read_dir_with_file_types(&self, path: &str) -> Result<Option<Vec<DirEntry>>> {
        let Some(resolved) = self.resolve_symlinks(&normalize(path), 0).await? else {
            return Ok(None);
        };
        match self.lstat(&resolved).await? {
            Some(s) if s.kind == EntryType::Directory => {}
            _ => return Ok(None),
        }
        let cursor = self.sql.exec(
            &format!(
                "SELECT name, type FROM {} WHERE parent_path = ? AND path != '/' ORDER BY name",
                self.table
            ),
            Some(vec![resolved.into()]),
        )?;
        let mut out = Vec::new();
        for row in cursor.next::<DirEntryRow>() {
            let row = row?;
            let Some(kind) = EntryType::parse(&row.r#type) else {
                continue;
            };
            out.push(DirEntry {
                name: row.name,
                kind,
            });
        }
        Ok(Some(out))
    }

    pub async fn mkdir(&self, path: &str, opts: MkdirOptions) -> Result<()> {
        let p = normalize(path);
        if p == "/" {
            return Ok(());
        }
        if opts.recursive {
            return self.mkdir_recursive(&p);
        }
        let parent = parent_path(&p);
        if !parent.is_empty()
            && parent != "/"
            && exec_count(
                &self.sql,
                &format!(
                    "SELECT 1 AS c FROM {} WHERE path = ? AND type = 'directory' LIMIT 1",
                    self.table
                ),
                Some(vec![parent.clone().into()]),
            )? == 0
        {
            return Err(worker::Error::RustError(format!(
                "mkdir: parent {parent} does not exist"
            )));
        }
        self.insert_dir(&p)
    }

    fn mkdir_recursive(&self, path: &str) -> Result<()> {
        let mut acc = String::new();
        for seg in path.split('/').filter(|s| !s.is_empty()) {
            acc.push('/');
            acc.push_str(seg);
            self.insert_dir(&acc)?;
        }
        Ok(())
    }

    fn insert_dir(&self, path: &str) -> Result<()> {
        let parent = parent_path(path);
        let name = path_name(path);
        self.sql.exec(
            &format!(
                "INSERT INTO {table} (path, parent_path, name, type, mime_type) \
                 VALUES (?, ?, ?, 'directory', 'inode/directory') \
                 ON CONFLICT(path) DO NOTHING",
                table = self.table
            ),
            Some(vec![path.to_string().into(), parent.into(), name.into()]),
        )?;
        Ok(())
    }

    fn ensure_parent_dirs(&self, file_path: &str) -> Result<()> {
        let parent = parent_path(file_path);
        if parent.is_empty() || parent == "/" {
            return Ok(());
        }
        self.mkdir_recursive(&parent)
    }

    pub async fn rm(&self, path: &str, opts: RmOptions) -> Result<()> {
        let p = normalize(path);
        let Some(stat) = self.lstat(&p).await? else {
            if opts.force {
                return Ok(());
            }
            return Err(worker::Error::RustError(format!("rm: {p} not found")));
        };
        match stat.kind {
            EntryType::File | EntryType::Symlink => self.rm_single(&p).await,
            EntryType::Directory => {
                if !opts.recursive {
                    let n = exec_count(
                        &self.sql,
                        &format!(
                            "SELECT count(*) AS c FROM {} WHERE parent_path = ?",
                            self.table
                        ),
                        Some(vec![p.clone().into()]),
                    )?;
                    if n > 0 {
                        return Err(worker::Error::RustError(format!(
                            "rm: {p} is non-empty and recursive=false"
                        )));
                    }
                }
                // Recursive: collect descendants by path prefix, delete each
                // (so R2 spills also get cleaned). Then delete the dir.
                let prefix = if p == "/" {
                    "/".to_string()
                } else {
                    format!("{p}/")
                };
                let cursor = self.sql.exec(
                    &format!(
                        "SELECT path FROM {} WHERE path = ? OR path LIKE ?",
                        self.table
                    ),
                    Some(vec![p.clone().into(), format!("{prefix}%").into()]),
                )?;
                let mut to_delete = Vec::new();
                for row in cursor.next::<PathRow>() {
                    to_delete.push(row?.path);
                }
                for child in to_delete {
                    self.rm_single(&child).await?;
                }
                Ok(())
            }
        }
    }

    async fn rm_single(&self, path: &str) -> Result<()> {
        // Pull the row first so we know whether to clean an R2 key.
        if let Some(r2) = &self.r2 {
            let cursor = self.sql.exec(
                &format!(
                    "SELECT storage_backend, r2_key FROM {} WHERE path = ? LIMIT 1",
                    self.table
                ),
                Some(vec![path.to_string().into()]),
            )?;
            if let Some(Ok(row)) = cursor.next::<R2RefRow>().next() {
                if row.storage_backend == "r2" {
                    if let Some(key) = row.r2_key {
                        let _ = r2.delete(&key).await;
                    }
                }
            }
        }
        self.sql.exec(
            &format!("DELETE FROM {} WHERE path = ?", self.table),
            Some(vec![path.to_string().into()]),
        )?;
        Ok(())
    }

    pub async fn cp(&self, src: &str, dst: &str, opts: CpOptions) -> Result<()> {
        let src = normalize(src);
        let dst = normalize(dst);
        let Some(src_stat) = self.lstat(&src).await? else {
            return Err(worker::Error::RustError(format!("cp: {src} not found")));
        };
        match src_stat.kind {
            EntryType::File => {
                let bytes = self.read_file_bytes(&src).await?.unwrap_or_default();
                self.write_file_bytes(&dst, &bytes).await
            }
            EntryType::Symlink => {
                let target = self.readlink(&src).await?.unwrap_or_default();
                self.symlink(&target, &dst).await
            }
            EntryType::Directory => {
                if !opts.recursive {
                    return Err(worker::Error::RustError(format!(
                        "cp: {src} is a directory and recursive=false"
                    )));
                }
                self.mkdir(&dst, MkdirOptions { recursive: true }).await?;
                let entries = self
                    .read_dir_with_file_types(&src)
                    .await?
                    .unwrap_or_default();
                for e in entries {
                    let s = format!("{src}/{}", e.name);
                    let d = format!("{dst}/{}", e.name);
                    Box::pin(self.cp(&s, &d, CpOptions { recursive: true })).await?;
                }
                Ok(())
            }
        }
    }

    pub async fn mv(&self, src: &str, dst: &str) -> Result<()> {
        let src = normalize(src);
        let dst = normalize(dst);
        let Some(src_stat) = self.lstat(&src).await? else {
            return Err(worker::Error::RustError(format!("mv: {src} not found")));
        };
        match src_stat.kind {
            EntryType::Directory => {
                // No bulk rename in SQL on parent_path; do cp -r + rm -r.
                self.cp(&src, &dst, CpOptions { recursive: true }).await?;
                self.rm(
                    &src,
                    RmOptions {
                        recursive: true,
                        force: true,
                    },
                )
                .await
            }
            _ => {
                // Single row: cp + rm preserves R2 keys correctly because
                // write_inner allocates a fresh r2_key for the new path.
                self.cp(&src, &dst, CpOptions::default()).await?;
                self.rm_single(&src).await
            }
        }
    }

    pub async fn symlink(&self, target: &str, link_path: &str) -> Result<()> {
        if target.len() > 4096 {
            return Err(worker::Error::RustError(format!(
                "symlink: target length {} exceeds 4096",
                target.len()
            )));
        }
        let p = normalize(link_path);
        self.ensure_parent_dirs(&p)?;
        let parent = parent_path(&p);
        let name = path_name(&p);
        self.sql.exec(
            &format!(
                "INSERT INTO {table} \
                   (path, parent_path, name, type, target, mime_type, modified_at) \
                 VALUES (?, ?, ?, 'symlink', ?, 'inode/symlink', unixepoch()) \
                 ON CONFLICT(path) DO UPDATE SET \
                   target = excluded.target, \
                   type = 'symlink', \
                   modified_at = unixepoch()",
                table = self.table
            ),
            Some(vec![
                p.into(),
                parent.into(),
                name.into(),
                target.to_string().into(),
            ]),
        )?;
        Ok(())
    }

    pub async fn readlink(&self, path: &str) -> Result<Option<String>> {
        let p = normalize(path);
        let cursor = self.sql.exec(
            &format!(
                "SELECT target FROM {} WHERE path = ? AND type = 'symlink' LIMIT 1",
                self.table
            ),
            Some(vec![p.into()]),
        )?;
        match cursor.next::<TargetRow>().next() {
            Some(Ok(r)) => Ok(r.target),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub async fn realpath(&self, path: &str) -> Result<Option<String>> {
        self.resolve_symlinks(&normalize(path), 0).await
    }

    /// Glob over the index using SQL LIKE. `*` → `%`, `?` → `_`.
    /// Returns absolute paths matching the pattern, sorted.
    pub async fn glob(&self, pattern: &str) -> Result<Vec<String>> {
        let like = glob_to_like(pattern);
        let cursor = self.sql.exec(
            &format!(
                "SELECT path FROM {} WHERE path LIKE ? ESCAPE '\\' ORDER BY path",
                self.table
            ),
            Some(vec![like.into()]),
        )?;
        let mut out = Vec::new();
        for row in cursor.next::<PathRow>() {
            out.push(row?.path);
        }
        Ok(out)
    }

    /// Follow symlinks down to a non-symlink target. Returns Ok(None)
    /// on ENOENT anywhere in the chain.
    async fn resolve_symlinks(&self, path: &str, depth: u32) -> Result<Option<String>> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(worker::Error::RustError(format!(
                "symlink: depth > {MAX_SYMLINK_DEPTH} resolving {path}"
            )));
        }
        let cursor = self.sql.exec(
            &format!(
                "SELECT type, target FROM {} WHERE path = ? LIMIT 1",
                self.table
            ),
            Some(vec![path.to_string().into()]),
        )?;
        let row = match cursor.next::<TypeTargetRow>().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e),
            None => return Ok(None),
        };
        if row.r#type != "symlink" {
            return Ok(Some(path.to_string()));
        }
        let Some(target) = row.target else {
            return Ok(Some(path.to_string()));
        };
        // Relative targets resolve against the link's parent. Absolute
        // (leading /) replace the path entirely.
        let next = if target.starts_with('/') {
            normalize(&target)
        } else {
            normalize(&format!("{}/{}", parent_path(path), target))
        };
        Box::pin(self.resolve_symlinks(&next, depth + 1)).await
    }

    fn r2_key(&self, path: &str) -> String {
        format!("{}/{}{}", self.r2_prefix, self.namespace, path)
    }
}

fn glob_to_like(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    for ch in pattern.chars() {
        match ch {
            '*' => out.push('%'),
            '?' => out.push('_'),
            // SQL LIKE meta-characters need escaping under ESCAPE '\\'.
            '%' | '_' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            c => out.push(c),
        }
    }
    out
}

fn exec_count(
    sql: &SqlStorage,
    query: &str,
    bindings: Option<Vec<worker::SqlStorageValue>>,
) -> Result<u64> {
    let cursor = sql.exec(query, bindings)?;
    Ok(cursor
        .next::<CountRow>()
        .next()
        .and_then(|r| r.ok())
        .map(|r| r.c)
        .unwrap_or(0))
}

// ── row types for SqlStorage::exec().next::<T>() ────────────────────────

use serde::Deserialize;

#[derive(Deserialize)]
struct CountRow {
    c: u64,
}

#[derive(Deserialize)]
struct StatRow {
    r#type: String,
    size: i64,
    modified_at: i64,
    mime_type: String,
}

#[derive(Deserialize)]
struct FileRow {
    storage_backend: String,
    content_encoding: String,
    content: Option<String>,
    r2_key: Option<String>,
}

#[derive(Deserialize)]
struct DirEntryRow {
    name: String,
    r#type: String,
}

#[derive(Deserialize)]
struct PathRow {
    path: String,
}

#[derive(Deserialize)]
struct TargetRow {
    target: Option<String>,
}

#[derive(Deserialize)]
struct TypeTargetRow {
    r#type: String,
    target: Option<String>,
}

#[derive(Deserialize)]
struct R2RefRow {
    storage_backend: String,
    r2_key: Option<String>,
}

// Keep EntryType::as_str linkable even when only the constructors are
// used externally; future code that builds INSERT statements from an
// EntryType value will want it.
#[allow(dead_code)]
fn _entry_type_kept_for_completeness(e: EntryType) -> &'static str {
    e.as_str()
}
