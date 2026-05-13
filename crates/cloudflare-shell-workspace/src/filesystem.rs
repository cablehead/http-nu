//! Rust port of @cloudflare/shell's `Workspace` class (upstream:
//! `filesystem.ts`).
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
//!     wrap in their own adapter.

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use futures_util::stream::{self, Stream, StreamExt};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use worker::{Bucket, SqlStorage};

use cloudflare_shell::{
    error::FsError,
    interface::{
        CpOptions, DirEntry, EntryType, FileSystem, MkdirOptions, OnChange, RmOptions, Stat,
        WorkspaceChangeEvent, WorkspaceChangeType, DEFAULT_BYTES_MIME, DEFAULT_TEXT_MIME,
        MAX_PATH_LENGTH, MAX_STREAM_SIZE, MAX_SYMLINK_DEPTH,
    },
    path_utils::{normalize, normalize_path, parent_path, path_name},
    Result,
};

use crate::schema;

/// Stream of file content chunks. Returned by
/// [`Workspace::read_file_stream`]. Items are owned `Vec<u8>` chunks so
/// the inline-bytes and R2-spilled paths share one item type.
pub type ReadStream = Pin<Box<dyn Stream<Item = Result<Vec<u8>>> + Unpin>>;

/// Upstream: filesystem.ts:1406 `getWorkspaceInfo()` return shape.
/// Aggregated counters across every row in the workspace table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkspaceInfo {
    pub file_count: u64,
    pub directory_count: u64,
    pub total_bytes: u64,
    pub r2_file_count: u64,
}

/// R2 spill threshold. Files larger than this go to R2; smaller stay
/// inline in the SQL row. Matches `@cloudflare/shell`'s
/// `inlineThreshold` default.
const R2_SPILL_THRESHOLD: usize = 1_500_000;

/// One Workspace = one user's filesystem. Cheap to construct;
/// `bootstrap()` is idempotent and only does work the first time.
///
/// Deliberately not `Debug`/`Clone`: `on_change` is `Arc<dyn Fn>` which
/// doesn't implement `Debug`, and Workspace today isn't formatted or
/// cloned anywhere. Restore the derives (with manual `Debug`) if a use
/// case appears.
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
    /// Upstream: filesystem.ts:232 `private readonly onChange`.
    /// Per-instance listener fired after every successful mutation.
    /// Wrapped in `Mutex` so `set_on_change` can take `&self`, letting
    /// the conformance harness call it through a `&F` reference.
    on_change: std::sync::Mutex<Option<OnChange>>,
    /// Port-only: forward-compat toggle for `write_file_stream`'s
    /// large-upload path. See `set_streaming_writes`.
    streaming_writes: AtomicBool,
}

/// Upstream: filesystem.ts:189 `const VALID_NAMESPACE = /^[a-zA-Z][a-zA-Z0-9_]*$/`.
///
/// The namespace flows directly into table/index identifiers via
/// `format!("cf_workspace_{namespace}")` and `format!("idx_..._...")`,
/// which means SqlStorage's parameter-binding can't help us -- the
/// only correct defense is to refuse anything that isn't a plain SQL
/// identifier suffix. Anything else is a code-injection vector
/// (e.g. a namespace of `x; DROP TABLE cf_workspace_default; --`
/// would break out of the table-name position in CREATE / SELECT /
/// DELETE statements).
fn is_valid_namespace(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl Workspace {
    /// Upstream: filesystem.ts:237 `constructor(options: WorkspaceOptions)`.
    /// We take `(sql, r2, namespace)` instead of an options bag --
    /// `onChange` callback (TS L108) is not yet wired.
    ///
    /// Rejects namespaces that don't match
    /// `/^[a-zA-Z][a-zA-Z0-9_]*$/` (upstream's `VALID_NAMESPACE`).
    /// The check is non-negotiable: this string ends up inline in
    /// CREATE TABLE / SELECT statements, so anything weaker is a SQL
    /// injection vector. See `is_valid_namespace`'s doc for why
    /// parameter binding isn't an option here.
    pub fn new(sql: SqlStorage, r2: Option<Bucket>, namespace: &str) -> Result<Self> {
        if !is_valid_namespace(namespace) {
            return Err(FsError::Other(format!(
                "invalid workspace namespace \"{namespace}\": must start with a letter and contain only alphanumeric characters or underscores"
            )));
        }
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
            on_change: std::sync::Mutex::new(None),
            streaming_writes: AtomicBool::new(false),
        };
        ws.bootstrap()?;
        Ok(ws)
    }

    /// Upstream: filesystem.ts:108 `WorkspaceOptions.onChange`.
    /// Per-instance listener; set after construction (TS sets it inside
    /// the constructor via the options bag). The callback fires after a
    /// successful mutation. Takes `&self` rather than `&mut self` so
    /// callers (including the conformance harness) can install
    /// listeners through a shared reference.
    pub fn set_on_change(&self, cb: OnChange) {
        *self.on_change.lock().unwrap() = Some(cb);
    }

    /// Upstream: filesystem.ts:307 `private emit()`.
    /// Clones the Arc out of the lock before invoking so the user's
    /// callback runs without holding the mutex.
    fn emit(&self, kind: WorkspaceChangeType, path: &str, entry_type: EntryType) {
        let cb = self.on_change.lock().unwrap().clone();
        if let Some(cb) = cb {
            cb(WorkspaceChangeEvent {
                kind,
                path: path.to_string(),
                entry_type,
            });
        }
    }

    /// Port-only convenience constructor. Uses `DEFAULT_NAMESPACE = "default"`,
    /// mirroring the TS default. No upstream equivalent (TS callers pass
    /// `namespace` explicitly via `WorkspaceOptions`).
    pub fn default(sql: SqlStorage, r2: Option<Bucket>) -> Result<Self> {
        Self::new(sql, r2, schema::DEFAULT_NAMESPACE)
    }

    /// Port-only: whether this Workspace was constructed with an R2
    /// bucket bound. Tests use this to decide which invariants apply
    /// (R2 spill paths, R2-file-count assertions, etc.).
    pub fn has_r2(&self) -> bool {
        self.r2.is_some()
    }

    /// Port-only: forward-compat toggle for the large-upload code path of
    /// `write_file_stream`.
    ///
    /// - **OFF (default).** `write_file_stream` follows upstream
    ///   (filesystem.ts:907) byte-for-byte: drain the stream into a
    ///   `Vec<u8>`, error `EFBIG` past `MAX_STREAM_SIZE` (100 MB), then
    ///   delegate to `write_file_bytes`. Safe on Workers up to roughly
    ///   half the isolate memory ceiling.
    /// - **ON.** Cap is lifted; the stream is still buffered today, but
    ///   the contract is forward-compatible: a future change will swap
    ///   the buffered path for R2 multipart upload (chunk into 5 MB
    ///   parts, pipe each into `Bucket::create_multipart_upload`,
    ///   `upload_part`, `complete`). Memory peak then drops to one part.
    ///   Until that change lands, treat ON as "I am sure I have memory
    ///   headroom for this upload."
    ///
    /// Toggling between calls is fine; the value is read atomically.
    pub fn set_streaming_writes(&self, enabled: bool) {
        self.streaming_writes.store(enabled, Ordering::Relaxed);
    }

    /// Port-only: current state of the [`set_streaming_writes`] toggle.
    pub fn streaming_writes(&self) -> bool {
        self.streaming_writes.load(Ordering::Relaxed)
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

    /// Upstream: filesystem.ts:1028 `exists()`.
    pub async fn exists(&self, path: &str) -> Result<bool> {
        let p = normalize_path(path)?;
        let n = exec_count(
            &self.sql,
            &format!("SELECT 1 AS c FROM {} WHERE path = ? LIMIT 1", self.table),
            Some(vec![p.into()]),
        )?;
        Ok(n > 0)
    }

    /// Upstream: filesystem.ts:1017 `fileExists()`.
    /// Returns `true` only when the resolved path exists *and* is a
    /// file. Symlinks are followed (matches upstream's
    /// `resolveSymlink(normalizePath(...))` then `type === "file"`).
    pub async fn file_exists(&self, path: &str) -> Result<bool> {
        let p = normalize_path(path)?;
        let Some(resolved) = self.resolve_symlinks(&p, 0).await? else {
            return Ok(false);
        };
        let cursor = self.sql.exec(
            &format!("SELECT type FROM {} WHERE path = ? LIMIT 1", self.table),
            Some(vec![resolved.into()]),
        )?;
        match cursor.next::<TypeRow>().next() {
            Some(Ok(row)) => Ok(row.r#type == "file"),
            Some(Err(e)) => Err(e.into()),
            None => Ok(false),
        }
    }

    /// Upstream: filesystem.ts:500 `stat()`.
    /// Follows symlinks. Returns `Ok(None)` on ENOENT (TS: `Promise<FileStat | null>`).
    pub async fn stat(&self, path: &str) -> Result<Option<Stat>> {
        let p = normalize_path(path)?;
        let resolved = self.resolve_symlinks(&p, 0).await?;
        match resolved {
            Some(p) => self.lstat(&p).await,
            None => Ok(None),
        }
    }

    /// Upstream: filesystem.ts:475 `lstat()`.
    /// Does NOT follow symlinks. `Ok(None)` on ENOENT.
    pub async fn lstat(&self, path: &str) -> Result<Option<Stat>> {
        let p = normalize_path(path)?;
        let cursor = self.sql.exec(
            &format!(
                "SELECT type, size, modified_at, mime_type FROM {} WHERE path = ? LIMIT 1",
                self.table
            ),
            Some(vec![p.into()]),
        )?;
        let row = match cursor.next::<StatRow>().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e.into()),
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
            mode: Stat::mode_for(kind),
        }))
    }

    /// Upstream: filesystem.ts:526 `readFile()`.
    pub async fn read_file(&self, path: &str) -> Result<Option<String>> {
        let bytes = match self.read_file_bytes(path).await? {
            Some(b) => b,
            None => return Ok(None),
        };
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| FsError::InvalidEncoding(format!("readFile invalid utf8: {e}")))
    }

    /// Upstream: filesystem.ts:569 `readFileBytes()`.
    /// Resolves R2 spill transparently.
    pub async fn read_file_bytes(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let p = normalize_path(path)?;
        let Some(resolved) = self.resolve_symlinks(&p, 0).await? else {
            return Ok(None);
        };
        // SELECT without the `type = 'file'` filter so we can distinguish
        // EISDIR (caller asked to read a directory) from ENOENT.
        // Upstream throws `EISDIR: <path> is a directory` (filesystem.ts:544 / L587).
        let cursor = self.sql.exec(
            &format!(
                "SELECT type, storage_backend, content_encoding, content, r2_key \
                 FROM {} WHERE path = ? LIMIT 1",
                self.table
            ),
            Some(vec![resolved.clone().into()]),
        )?;
        let row = match cursor.next::<FileRow>().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e.into()),
            None => return Ok(None),
        };
        if row.r#type != "file" {
            return Err(FsError::IsDir(format!("{resolved} is a {}", row.r#type)));
        }
        let bytes = if row.storage_backend == "r2" {
            let Some(r2) = &self.r2 else {
                return Err(FsError::Io(format!(
                    "readFileBytes {resolved} is R2-backed but no R2 bucket bound"
                )));
            };
            let Some(key) = row.r2_key else {
                return Err(FsError::Io(format!(
                    "readFileBytes {resolved} storage_backend=r2 but r2_key is NULL"
                )));
            };
            let obj = match r2.get(&key).execute().await? {
                Some(o) => o,
                None => return Ok(None),
            };
            let body = obj
                .body()
                .ok_or_else(|| FsError::Io(format!("readFileBytes R2 object {key} has no body")))?;
            body.bytes().await?
        } else {
            // Inline. content_encoding='base64' for binary, anything else
            // treats `content` as utf8 text (matches @cloudflare/shell).
            let content = row.content.unwrap_or_default();
            if row.content_encoding == "base64" {
                B64.decode(content.as_bytes())
                    .map_err(|e| FsError::InvalidEncoding(format!("base64 decode: {e}")))?
            } else {
                content.into_bytes()
            }
        };
        Ok(Some(bytes))
    }

    /// Upstream: filesystem.ts:851 `readFileStream()`.
    ///
    /// Returns a `Stream<Item = Result<Vec<u8>>>`. The R2-spilled path
    /// proxies `Object::body().stream()` so bytes are not buffered. The
    /// inline path yields the row's content as a single chunk -- matches
    /// upstream's `new ReadableStream({ start(c) { c.enqueue(bytes); c.close(); }})`.
    ///
    /// `Ok(None)` on ENOENT (deviation: upstream returns `null` -> same
    /// shape). `EISDIR` on a directory. ENOENT for an R2 row whose key
    /// has been deleted out from under us collapses to `Ok(None)` as
    /// well (matches upstream's empty-stream emit at filesystem.ts:887).
    pub async fn read_file_stream(&self, path: &str) -> Result<Option<ReadStream>> {
        let p = normalize_path(path)?;
        let Some(resolved) = self.resolve_symlinks(&p, 0).await? else {
            return Ok(None);
        };
        let cursor = self.sql.exec(
            &format!(
                "SELECT type, storage_backend, content_encoding, content, r2_key \
                 FROM {} WHERE path = ? LIMIT 1",
                self.table
            ),
            Some(vec![resolved.clone().into()]),
        )?;
        let row = match cursor.next::<FileRow>().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e.into()),
            None => return Ok(None),
        };
        if row.r#type != "file" {
            return Err(FsError::IsDir(format!("{resolved} is a {}", row.r#type)));
        }
        if row.storage_backend == "r2" {
            let Some(r2) = &self.r2 else {
                return Err(FsError::Io(format!(
                    "readFileStream {resolved} is R2-backed but no R2 bucket bound"
                )));
            };
            let Some(key) = row.r2_key else {
                return Err(FsError::Io(format!(
                    "readFileStream {resolved} storage_backend=r2 but r2_key is NULL"
                )));
            };
            let obj = match r2.get(&key).execute().await? {
                Some(o) => o,
                None => return Ok(None),
            };
            let Some(body) = obj.body() else {
                return Ok(None);
            };
            let bs = body.stream()?.map(|chunk| chunk.map_err(FsError::from));
            return Ok(Some(Box::pin(bs)));
        }
        let content = row.content.unwrap_or_default();
        let bytes: Vec<u8> = if row.content_encoding == "base64" {
            B64.decode(content.as_bytes())
                .map_err(|e| FsError::InvalidEncoding(format!("base64 decode: {e}")))?
        } else {
            content.into_bytes()
        };
        // `stream::iter` is Unpin (unlike `stream::once`, which wraps a
        // future) -- a one-shot Vec is enough to mirror upstream's
        // `enqueue(bytes); close();` shape.
        let once: ReadStream = Box::pin(stream::iter(std::iter::once(Ok(bytes))));
        Ok(Some(once))
    }

    /// Upstream: filesystem.ts:729 `writeFile(path, content, mimeType = "text/plain")`.
    /// `mime_type = None` defaults to `text/plain`, matching upstream.
    pub async fn write_file(
        &self,
        path: &str,
        content: &str,
        mime_type: Option<&str>,
    ) -> Result<()> {
        let mime = mime_type.unwrap_or(DEFAULT_TEXT_MIME);
        self.write_inner(path, content.as_bytes(), "utf8", mime)
            .await
    }

    /// Upstream: filesystem.ts:611 `writeFileBytes(path, data, mimeType = "application/octet-stream")`.
    /// R2 spill at 1.5MB. `mime_type = None` defaults to `application/octet-stream`,
    /// matching upstream.
    pub async fn write_file_bytes(
        &self,
        path: &str,
        content: &[u8],
        mime_type: Option<&str>,
    ) -> Result<()> {
        let mime = mime_type.unwrap_or(DEFAULT_BYTES_MIME);
        // Choose encoding based on utf8-validity. utf8-clean bytes stay
        // as TEXT (cheaper, queryable); binary goes base64 (matches
        // @cloudflare/shell's content_encoding semantics).
        if let Ok(s) = std::str::from_utf8(content) {
            self.write_inner(path, s.as_bytes(), "utf8", mime).await
        } else {
            self.write_inner(path, content, "base64", mime).await
        }
    }

    /// Upstream: filesystem.ts:907 `writeFileStream()`.
    ///
    /// Drain the stream into a `Vec<u8>` and delegate to
    /// [`write_file_bytes`](Self::write_file_bytes). Matches upstream's
    /// collect-then-write shape (the TS version buffers chunks then calls
    /// `writeFileBytes`).
    ///
    /// The `EFBIG` cap (`MAX_STREAM_SIZE`, 100 MB) is gated on
    /// [`streaming_writes`](Self::streaming_writes):
    /// - OFF (default): enforce the cap. Faithful to upstream.
    /// - ON: cap is lifted; caller is responsible for memory headroom.
    ///   Forward-compatible with the future multipart-upload streaming
    ///   path -- see [`set_streaming_writes`](Self::set_streaming_writes).
    pub async fn write_file_stream<S>(
        &self,
        path: &str,
        stream: S,
        mime_type: Option<&str>,
    ) -> Result<()>
    where
        S: Stream<Item = Result<Vec<u8>>> + Unpin,
    {
        let cap_enforced = !self.streaming_writes();
        let mut buf: Vec<u8> = Vec::new();
        let mut s = stream;
        while let Some(chunk) = s.next().await {
            let chunk = chunk?;
            if cap_enforced && buf.len() + chunk.len() > MAX_STREAM_SIZE {
                return Err(FsError::NoSpace(format!(
                    "writeFileStream stream exceeds maximum size of {MAX_STREAM_SIZE} bytes"
                )));
            }
            buf.extend_from_slice(&chunk);
        }
        self.write_file_bytes(path, &buf, mime_type).await
    }

    /// Upstream: filesystem.ts:938 `appendFile()`.
    /// Preserves the existing entry's `mime_type` (re-reads via `lstat`
    /// rather than overwriting with the default), matching the upstream
    /// semantic that appendFile shouldn't change Content-Type.
    pub async fn append_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let existing = self.read_file_bytes(path).await?.unwrap_or_default();
        let existing_mime = self.lstat(path).await?.map(|s| s.mime_type);
        let mut combined = Vec::with_capacity(existing.len() + content.len());
        combined.extend_from_slice(&existing);
        combined.extend_from_slice(content);
        self.write_file_bytes(path, &combined, existing_mime.as_deref())
            .await
    }

    async fn write_inner(
        &self,
        path: &str,
        content: &[u8],
        encoding: &str,
        mime_type: &str,
    ) -> Result<()> {
        let p = normalize_path(path)?;
        // Upstream: filesystem.ts:619 / L737 -- `EISDIR: cannot write to
        // root directory`.
        if p == "/" {
            return Err(FsError::IsDir("cannot write to root directory".to_string()));
        }
        self.ensure_parent_dirs(&p)?;
        let parent = parent_path(&p);
        let name = path_name(&p);
        let size = content.len() as i64;
        // Track Create vs Update for onChange emit (upstream emits
        // `existing ? "update" : "create"` at filesystem.ts:680 / 719 /
        // 801 / 841).
        let existed = exec_count(
            &self.sql,
            &format!("SELECT 1 AS c FROM {} WHERE path = ? LIMIT 1", self.table),
            Some(vec![p.clone().into()]),
        )? > 0;

        if content.len() > R2_SPILL_THRESHOLD {
            let Some(r2) = &self.r2 else {
                return Err(FsError::NoSpace(format!("writeFileBytes {p} would spill to R2 ({} bytes > {R2_SPILL_THRESHOLD}) but no R2 bucket bound",
                    content.len()
                )));
            };
            let key = self.r2_key(&p);
            r2.put(&key, content.to_vec()).execute().await?;
            self.sql.exec(
                &format!(
                    "INSERT INTO {table} \
                       (path, parent_path, name, type, mime_type, size, storage_backend, r2_key, \
                        content_encoding, content, modified_at) \
                     VALUES (?, ?, ?, 'file', ?, ?, 'r2', ?, ?, NULL, unixepoch()) \
                     ON CONFLICT(path) DO UPDATE SET \
                       mime_type = excluded.mime_type, \
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
                    mime_type.to_string().into(),
                    size.into(),
                    key.into(),
                    encoding.into(),
                ]),
            )?;
            self.emit(
                if existed {
                    WorkspaceChangeType::Update
                } else {
                    WorkspaceChangeType::Create
                },
                &p,
                EntryType::File,
            );
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
                   (path, parent_path, name, type, mime_type, size, storage_backend, r2_key, \
                    content_encoding, content, modified_at) \
                 VALUES (?, ?, ?, 'file', ?, ?, 'inline', NULL, ?, ?, unixepoch()) \
                 ON CONFLICT(path) DO UPDATE SET \
                   mime_type = excluded.mime_type, \
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
                mime_type.to_string().into(),
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
        self.emit(
            if existed {
                WorkspaceChangeType::Update
            } else {
                WorkspaceChangeType::Create
            },
            &p,
            EntryType::File,
        );
        Ok(())
    }

    /// Upstream: filesystem.ts:1041 `readDir()`.
    /// Names-only variant. See `read_dir_with_file_types` for entry metadata.
    pub async fn read_dir(&self, path: &str) -> Result<Option<Vec<String>>> {
        let Some(entries) = self.read_dir_with_file_types(path).await? else {
            return Ok(None);
        };
        Ok(Some(entries.into_iter().map(|e| e.name).collect()))
    }

    /// Port-only variant of `readDir`. Upstream returns `FileInfo[]` from
    /// `readDir` (filesystem.ts:1041); we split into names-only +
    /// with-file-types for ergonomics. Same SQL index hit either way.
    pub async fn read_dir_with_file_types(&self, path: &str) -> Result<Option<Vec<DirEntry>>> {
        let p = normalize_path(path)?;
        let Some(resolved) = self.resolve_symlinks(&p, 0).await? else {
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

    /// Upstream: filesystem.ts:1100 `mkdir()`.
    /// `MkdirOptions { recursive }` matches the TS option shape.
    pub async fn mkdir(&self, path: &str, opts: MkdirOptions) -> Result<()> {
        let p = normalize_path(path)?;
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
            return Err(FsError::NotFound(format!(
                "mkdir parent {parent} does not exist"
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
        // Upstream emits "create" only when a directory is actually new
        // (filesystem.ts:1157 / L1510 fire after the SQL has produced a
        // new row). The ON CONFLICT DO NOTHING above swallows the
        // no-op case, so we mirror by pre-checking existence.
        let existed = exec_count(
            &self.sql,
            &format!("SELECT 1 AS c FROM {} WHERE path = ? LIMIT 1", self.table),
            Some(vec![path.to_string().into()]),
        )? > 0;
        self.sql.exec(
            &format!(
                "INSERT INTO {table} (path, parent_path, name, type, mime_type) \
                 VALUES (?, ?, ?, 'directory', 'inode/directory') \
                 ON CONFLICT(path) DO NOTHING",
                table = self.table
            ),
            Some(vec![path.to_string().into(), parent.into(), name.into()]),
        )?;
        if !existed {
            self.emit(WorkspaceChangeType::Create, path, EntryType::Directory);
        }
        Ok(())
    }

    fn ensure_parent_dirs(&self, file_path: &str) -> Result<()> {
        let parent = parent_path(file_path);
        if parent.is_empty() || parent == "/" {
            return Ok(());
        }
        self.mkdir_recursive(&parent)
    }

    /// Upstream: filesystem.ts:1164 `rm()`.
    /// `RmOptions { recursive, force }` matches the TS option shape.
    /// Covers both file and directory paths -- no separate `deleteFile`.
    pub async fn rm(&self, path: &str, opts: RmOptions) -> Result<()> {
        let p = normalize_path(path)?;
        let Some(stat) = self.lstat(&p).await? else {
            if opts.force {
                return Ok(());
            }
            return Err(FsError::NotFound(format!("rm {p} not found")));
        };
        match stat.kind {
            EntryType::File | EntryType::Symlink => self.rm_single(&p, stat.kind).await,
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
                        return Err(FsError::NotEmpty(format!(
                            "rm {p} is non-empty and recursive=false"
                        )));
                    }
                }
                // Recursive: collect descendants by path prefix, delete each
                // (so R2 spills also get cleaned). Then delete the dir.
                // SELECT type too so each rm_single can emit the right
                // EntryType in its onChange Delete event.
                let prefix = if p == "/" {
                    "/".to_string()
                } else {
                    format!("{p}/")
                };
                let cursor = self.sql.exec(
                    &format!(
                        "SELECT path, type FROM {} WHERE path = ? OR path LIKE ?",
                        self.table
                    ),
                    Some(vec![p.clone().into(), format!("{prefix}%").into()]),
                )?;
                let mut to_delete: Vec<(String, EntryType)> = Vec::new();
                for row in cursor.next::<PathTypeRow>() {
                    let row = row?;
                    let Some(kind) = EntryType::parse(&row.r#type) else {
                        continue;
                    };
                    to_delete.push((row.path, kind));
                }
                for (child, kind) in to_delete {
                    self.rm_single(&child, kind).await?;
                }
                Ok(())
            }
        }
    }

    async fn rm_single(&self, path: &str, entry_type: EntryType) -> Result<()> {
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
        // Upstream emits after the delete (filesystem.ts:1012 / L1212).
        self.emit(WorkspaceChangeType::Delete, path, entry_type);
        Ok(())
    }

    /// Upstream: filesystem.ts:990 `deleteFile()`.
    ///
    /// File-or-symlink-only counterpart of [`rm`](Self::rm): refuses to
    /// delete a directory (use `rm` with `recursive=true` for that).
    /// Returns `Ok(false)` when the path doesn't exist; `Ok(true)` when
    /// a row was deleted. R2-backed rows have their object dropped too.
    ///
    /// Upstream raises `EISDIR: <path> is a directory -- use rm() instead`
    /// (filesystem.ts:1004) on a directory; we match.
    pub async fn delete_file(&self, path: &str) -> Result<bool> {
        let p = normalize_path(path)?;
        let cursor = self.sql.exec(
            &format!("SELECT type FROM {} WHERE path = ? LIMIT 1", self.table),
            Some(vec![p.clone().into()]),
        )?;
        let row = match cursor.next::<TypeRow>().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e.into()),
            None => return Ok(false),
        };
        if row.r#type == "directory" {
            return Err(FsError::IsDir(format!(
                "{p} is a directory -- use rm() instead"
            )));
        }
        let kind = EntryType::parse(&row.r#type)
            .ok_or_else(|| FsError::Io(format!("deleteFile: unknown row type {}", row.r#type)))?;
        self.rm_single(&p, kind).await?;
        Ok(true)
    }

    /// Upstream: filesystem.ts:1221 `cp()`.
    /// `CpOptions { recursive }` matches the TS option shape.
    pub async fn cp(&self, src: &str, dst: &str, opts: CpOptions) -> Result<()> {
        let src = normalize_path(src)?;
        let dst = normalize_path(dst)?;
        let Some(src_stat) = self.lstat(&src).await? else {
            return Err(FsError::NotFound(format!(
                "no such file or directory: {src}"
            )));
        };
        match src_stat.kind {
            EntryType::File => {
                // Preserve mime_type across cp (matches upstream
                // filesystem.ts:1255 which passes srcStat.mimeType).
                let bytes = self.read_file_bytes(&src).await?.unwrap_or_default();
                self.write_file_bytes(&dst, &bytes, Some(&src_stat.mime_type))
                    .await
            }
            EntryType::Symlink => {
                let target = self.readlink(&src).await?.unwrap_or_default();
                self.symlink(&target, &dst).await
            }
            EntryType::Directory => {
                if !opts.recursive {
                    return Err(FsError::IsDir(format!(
                        "cannot copy directory without recursive: {src}"
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

    /// Upstream: filesystem.ts:1264 `mv()`.
    pub async fn mv(&self, src: &str, dst: &str) -> Result<()> {
        let src = normalize_path(src)?;
        let dst = normalize_path(dst)?;
        let Some(src_stat) = self.lstat(&src).await? else {
            return Err(FsError::NotFound(format!(
                "no such file or directory: {src}"
            )));
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
                self.rm_single(&src, src_stat.kind).await
            }
        }
    }

    /// Upstream: filesystem.ts:415 `symlink()`. `MAX_SYMLINK_DEPTH = 40`.
    pub async fn symlink(&self, target: &str, link_path: &str) -> Result<()> {
        if target.len() > MAX_PATH_LENGTH {
            return Err(FsError::NameTooLong(format!(
                "symlink target length {} exceeds {MAX_PATH_LENGTH}",
                target.len()
            )));
        }
        let p = normalize_path(link_path)?;
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
                p.clone().into(),
                parent.into(),
                name.into(),
                target.to_string().into(),
            ]),
        )?;
        // Upstream emits "create" unconditionally for symlink
        // (filesystem.ts:457), even if ON CONFLICT replaced an existing
        // symlink. Match that.
        self.emit(WorkspaceChangeType::Create, &p, EntryType::Symlink);
        Ok(())
    }

    /// Upstream: filesystem.ts:460 `readlink()`.
    pub async fn readlink(&self, path: &str) -> Result<Option<String>> {
        let p = normalize_path(path)?;
        let cursor = self.sql.exec(
            &format!(
                "SELECT target FROM {} WHERE path = ? AND type = 'symlink' LIMIT 1",
                self.table
            ),
            Some(vec![p.into()]),
        )?;
        match cursor.next::<TargetRow>().next() {
            Some(Ok(r)) => Ok(r.target),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    /// Port-only public helper. Upstream resolves symlinks inline inside
    /// methods that need it (e.g. `stat`, `readFile`); we surface it
    /// because callers (e.g. `crate::cf::snapshot_vfs::SnapshotVfs`) want the
    /// resolved path directly.
    pub async fn realpath(&self, path: &str) -> Result<Option<String>> {
        let p = normalize_path(path)?;
        self.resolve_symlinks(&p, 0).await
    }

    /// Upstream: filesystem.ts:1071 `glob()`.
    /// Glob over the index using SQL LIKE. `*` -> `%`, `?` -> `_`.
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

    /// Upstream: filesystem.ts:1406 `getWorkspaceInfo()`.
    /// Aggregate counters: file count, directory count, total bytes
    /// (files only -- directories have `size = 0`), and the subset of
    /// files that have spilled to R2. Single `SUM(CASE ...)` scan over
    /// the index table; same query shape as upstream.
    pub async fn get_workspace_info(&self) -> Result<WorkspaceInfo> {
        let cursor = self.sql.exec(
            &format!(
                "SELECT \
                    SUM(CASE WHEN type = 'file'                            THEN 1 ELSE 0 END) AS files, \
                    SUM(CASE WHEN type = 'directory'                       THEN 1 ELSE 0 END) AS dirs, \
                    COALESCE(SUM(CASE WHEN type = 'file' THEN size ELSE 0 END), 0)            AS total, \
                    SUM(CASE WHEN type = 'file' AND storage_backend = 'r2' THEN 1 ELSE 0 END) AS r2files \
                 FROM {}",
                self.table
            ),
            None,
        )?;
        let row = match cursor.next::<InfoRow>().next() {
            Some(Ok(r)) => r,
            Some(Err(e)) => return Err(e.into()),
            None => return Ok(WorkspaceInfo::default()),
        };
        Ok(WorkspaceInfo {
            file_count: row.files.max(0) as u64,
            directory_count: row.dirs.max(0) as u64,
            total_bytes: row.total.max(0) as u64,
            r2_file_count: row.r2files.max(0) as u64,
        })
    }

    /// Follow symlinks down to a non-symlink target. Returns Ok(None)
    /// on ENOENT anywhere in the chain.
    async fn resolve_symlinks(&self, path: &str, depth: u32) -> Result<Option<String>> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(FsError::SymlinkLoop(format!(
                "too many symbolic links (>{MAX_SYMLINK_DEPTH}) resolving {path}"
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
            Some(Err(e)) => return Err(e.into()),
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
    r#type: String,
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
struct PathTypeRow {
    path: String,
    r#type: String,
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

#[derive(Deserialize)]
struct TypeRow {
    r#type: String,
}

#[derive(Deserialize)]
struct InfoRow {
    files: i64,
    dirs: i64,
    total: i64,
    r2files: i64,
}

// ── impl FileSystem ──────────────────────────────────────────────────
//
// Workspace's inherent methods already have the right signatures and
// semantics; this impl block exists so callers can be polymorphic via
// `<F: FileSystem>` and so the conformance suite in
// `cloudflare_shell::conformance` can run against the real DO-backed FS.
// Each method delegates straight to the inherent fn -- no behavioural
// divergence between the trait route and the inherent route.

impl FileSystem for Workspace {
    async fn exists(&self, path: &str) -> Result<bool> {
        Workspace::exists(self, path).await
    }

    async fn stat(&self, path: &str) -> Result<Option<Stat>> {
        Workspace::stat(self, path).await
    }

    async fn lstat(&self, path: &str) -> Result<Option<Stat>> {
        Workspace::lstat(self, path).await
    }

    async fn read_file(&self, path: &str) -> Result<Option<String>> {
        Workspace::read_file(self, path).await
    }

    async fn read_file_bytes(&self, path: &str) -> Result<Option<Vec<u8>>> {
        Workspace::read_file_bytes(self, path).await
    }

    async fn write_file(&self, path: &str, content: &str, mime_type: Option<&str>) -> Result<()> {
        Workspace::write_file(self, path, content, mime_type).await
    }

    async fn write_file_bytes(
        &self,
        path: &str,
        content: &[u8],
        mime_type: Option<&str>,
    ) -> Result<()> {
        Workspace::write_file_bytes(self, path, content, mime_type).await
    }

    async fn append_file(&self, path: &str, content: &[u8]) -> Result<()> {
        Workspace::append_file(self, path, content).await
    }

    async fn read_dir(&self, path: &str) -> Result<Option<Vec<String>>> {
        Workspace::read_dir(self, path).await
    }

    async fn read_dir_with_file_types(&self, path: &str) -> Result<Option<Vec<DirEntry>>> {
        Workspace::read_dir_with_file_types(self, path).await
    }

    async fn mkdir(&self, path: &str, opts: MkdirOptions) -> Result<()> {
        Workspace::mkdir(self, path, opts).await
    }

    async fn rm(&self, path: &str, opts: RmOptions) -> Result<()> {
        Workspace::rm(self, path, opts).await
    }

    async fn cp(&self, src: &str, dst: &str, opts: CpOptions) -> Result<()> {
        Workspace::cp(self, src, dst, opts).await
    }

    async fn mv(&self, src: &str, dst: &str) -> Result<()> {
        Workspace::mv(self, src, dst).await
    }

    async fn symlink(&self, target: &str, link_path: &str) -> Result<()> {
        Workspace::symlink(self, target, link_path).await
    }

    async fn readlink(&self, path: &str) -> Result<Option<String>> {
        Workspace::readlink(self, path).await
    }

    async fn realpath(&self, path: &str) -> Result<Option<String>> {
        Workspace::realpath(self, path).await
    }

    async fn glob(&self, pattern: &str) -> Result<Vec<String>> {
        Workspace::glob(self, pattern).await
    }
}

#[cfg(test)]
mod tests {
    use super::is_valid_namespace;

    #[test]
    fn valid_namespaces_accepted() {
        for ns in ["a", "default", "user1", "User_2", "x__y__z", "z9"] {
            assert!(is_valid_namespace(ns), "expected valid: {ns:?}");
        }
    }

    #[test]
    fn invalid_namespaces_rejected() {
        // Leading non-letter, special chars, empty, SQL-injection shapes.
        for ns in [
            "",
            "1abc",                                 // starts with digit
            "_underscore",                          // starts with underscore
            "with-hyphen",                          // hyphen
            "with space",                           // whitespace
            "x.y",                                  // dot
            "x;DROP TABLE cf_workspace_default;--", // classic injection
            "unicode\u{00e9}",                      // non-ascii
            "\"quoted\"",                           // quotes
        ] {
            assert!(!is_valid_namespace(ns), "expected invalid: {ns:?}");
        }
    }
}
