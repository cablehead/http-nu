//! Upstream: `.src/agents/packages/shell/src/fs/in-memory-fs.ts`.
//!
//! Pure-Rust in-memory `FileSystem` impl. Same `Ok(None)`-on-ENOENT
//! convention as `Workspace`, same `Stat` / `DirEntry` / option types,
//! same `set_on_change` listener wiring (port-only -- upstream
//! `InMemoryFs` does not emit; we add it so test code can subscribe to
//! the same change stream regardless of backend).
//!
//! Storage is a flat `HashMap<String, Node>` keyed by absolute path
//! rather than upstream's `VDirNode` tree. Semantic parity, simpler
//! representation.
//!
//! NOT ported from upstream (deferred):
//!   - Lazy file entries (`LazyFileProvider`, `forceLazy`).
//!   - Sync helpers (`writeFileSync`, `mkdirSync`, `writeFileLazy`).
//!   - `chmod` / `utimes` / `link`.
//!   - `InitialFiles` constructor option.
//!
//! IMPORTANT (mock-divergence warning): InMemoryFs is a *behavioural
//! double*, not the real storage. Tests that rely on Workspace-only
//! properties (R2 spill, SQLite-specific semantics, DO persistence
//! across eviction) MUST run against `Workspace` directly via wrangler
//! dev / cf:deploy. The conformance discipline in
//! `crate::shell::conformance` is what keeps the two impls honest.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::shell::{
    error::{FsError, Result},
    interface::{
        CpOptions, DirEntry, EntryType, FileSystem, MkdirOptions, OnChange, RmOptions, Stat,
        WorkspaceChangeEvent, WorkspaceChangeType, DEFAULT_BYTES_MIME, DEFAULT_TEXT_MIME,
        MAX_PATH_LENGTH, MAX_SYMLINK_DEPTH,
    },
    path_utils::{normalize_path, parent_path, path_name},
};

/// One in-memory filesystem. `Send + Sync` via the inner `Mutex` so the
/// type can sit behind an `Arc` when needed.
pub struct InMemoryFs {
    inner: Mutex<Inner>,
}

struct Inner {
    nodes: HashMap<String, Node>,
    on_change: Option<OnChange>,
}

#[derive(Debug, Clone)]
enum Node {
    File {
        bytes: Vec<u8>,
        mime_type: String,
        modified_at: i64,
    },
    Directory {
        modified_at: i64,
    },
    Symlink {
        target: String,
        modified_at: i64,
    },
}

impl Node {
    fn kind(&self) -> EntryType {
        match self {
            Node::File { .. } => EntryType::File,
            Node::Directory { .. } => EntryType::Directory,
            Node::Symlink { .. } => EntryType::Symlink,
        }
    }

    fn size(&self) -> u64 {
        match self {
            Node::File { bytes, .. } => bytes.len() as u64,
            Node::Directory { .. } => 0,
            Node::Symlink { target, .. } => target.len() as u64,
        }
    }

    fn modified_at(&self) -> i64 {
        match self {
            Node::File { modified_at, .. }
            | Node::Directory { modified_at }
            | Node::Symlink { modified_at, .. } => *modified_at,
        }
    }

    fn mime_type(&self) -> &str {
        match self {
            Node::File { mime_type, .. } => mime_type,
            Node::Directory { .. } => "inode/directory",
            Node::Symlink { .. } => "inode/symlink",
        }
    }
}

#[cfg(all(feature = "cloudflare", target_arch = "wasm32"))]
fn now_secs() -> i64 {
    (worker::Date::now().as_millis() / 1000) as i64
}

#[cfg(not(all(feature = "cloudflare", target_arch = "wasm32")))]
fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Default for InMemoryFs {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryFs {
    /// Upstream: fs/in-memory-fs.ts:119 `constructor(initialFiles?)`.
    /// Seeds a root directory entry so `exists("/")` and `stat("/")`
    /// work without special-casing.
    pub fn new() -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(
            "/".to_string(),
            Node::Directory {
                modified_at: now_secs(),
            },
        );
        Self {
            inner: Mutex::new(Inner {
                nodes,
                on_change: None,
            }),
        }
    }

    /// Port-only convenience. Upstream `InMemoryFs` doesn't expose an
    /// `onChange` listener (only `Workspace` does); we surface it for
    /// API symmetry so test code can subscribe to the same
    /// `WorkspaceChangeEvent` stream regardless of which FS backs it.
    pub fn set_on_change(&self, cb: OnChange) {
        self.lock().on_change = Some(cb);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn emit(&self, kind: WorkspaceChangeType, path: &str, entry_type: EntryType) {
        if let Some(cb) = self.lock().on_change.clone() {
            cb(WorkspaceChangeEvent {
                kind,
                path: path.to_string(),
                entry_type,
            });
        }
    }

    fn ensure_parent_dirs(inner: &mut Inner, path: &str) -> Result<()> {
        let mut acc = String::new();
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segs.len() <= 1 {
            return Ok(());
        }
        for seg in &segs[..segs.len() - 1] {
            acc.push('/');
            acc.push_str(seg);
            match inner.nodes.get(&acc) {
                Some(Node::Directory { .. }) => {}
                Some(other) => {
                    return Err(FsError::NotDir(format!(
                        "not a directory: {} (in path {path})",
                        kind_name(other.kind())
                    )))
                }
                None => {
                    inner.nodes.insert(
                        acc.clone(),
                        Node::Directory {
                            modified_at: now_secs(),
                        },
                    );
                }
            }
        }
        Ok(())
    }

    fn resolve_symlinks(inner: &Inner, path: &str, depth: u32) -> Result<Option<String>> {
        if depth > MAX_SYMLINK_DEPTH {
            return Err(FsError::SymlinkLoop(format!(
                "too many symbolic links (>{MAX_SYMLINK_DEPTH}) resolving {path}"
            )));
        }
        match inner.nodes.get(path) {
            None => Ok(None),
            Some(Node::Symlink { target, .. }) => {
                let resolved = if target.starts_with('/') {
                    target.clone()
                } else {
                    format!("{}/{}", parent_path(path), target)
                };
                let resolved = normalize_path(&resolved)?;
                Self::resolve_symlinks(inner, &resolved, depth + 1)
            }
            Some(_) => Ok(Some(path.to_string())),
        }
    }

    fn write_inner(&self, path: &str, content: &[u8], mime_type: &str) -> Result<()> {
        let p = normalize_path(path)?;
        if p == "/" {
            return Err(FsError::IsDir(
                "cannot write to root directory".to_string(),
            ));
        }
        let kind = {
            let mut inner = self.lock();
            Self::ensure_parent_dirs(&mut inner, &p)?;
            if let Some(existing) = inner.nodes.get(&p) {
                if !matches!(existing, Node::File { .. }) {
                    return Err(FsError::IsDir(format!(
                        "cannot overwrite {} at {p}",
                        kind_name(existing.kind())
                    )));
                }
            }
            let existed = inner.nodes.contains_key(&p);
            inner.nodes.insert(
                p.clone(),
                Node::File {
                    bytes: content.to_vec(),
                    mime_type: mime_type.to_string(),
                    modified_at: now_secs(),
                },
            );
            if existed {
                WorkspaceChangeType::Update
            } else {
                WorkspaceChangeType::Create
            }
        };
        self.emit(kind, &p, EntryType::File);
        Ok(())
    }
}

impl FileSystem for InMemoryFs {
    async fn exists(&self, path: &str) -> Result<bool> {
        let p = normalize_path(path)?;
        Ok(self.lock().nodes.contains_key(&p))
    }

    async fn stat(&self, path: &str) -> Result<Option<Stat>> {
        let p = normalize_path(path)?;
        let inner = self.lock();
        let Some(resolved) = Self::resolve_symlinks(&inner, &p, 0)? else {
            return Ok(None);
        };
        Ok(inner.nodes.get(&resolved).map(make_stat))
    }

    async fn lstat(&self, path: &str) -> Result<Option<Stat>> {
        let p = normalize_path(path)?;
        Ok(self.lock().nodes.get(&p).map(make_stat))
    }

    async fn read_file(&self, path: &str) -> Result<Option<String>> {
        let bytes = match self.read_file_bytes(path).await? {
            Some(b) => b,
            None => return Ok(None),
        };
        String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| FsError::InvalidEncoding(format!("readFile invalid utf8: {e}")))
    }

    async fn read_file_bytes(&self, path: &str) -> Result<Option<Vec<u8>>> {
        let p = normalize_path(path)?;
        let inner = self.lock();
        let Some(resolved) = Self::resolve_symlinks(&inner, &p, 0)? else {
            return Ok(None);
        };
        match inner.nodes.get(&resolved) {
            None => Ok(None),
            Some(Node::File { bytes, .. }) => Ok(Some(bytes.clone())),
            Some(other) => Err(FsError::IsDir(format!(
                "{resolved} is a {}",
                kind_name(other.kind())
            ))),
        }
    }

    async fn write_file(
        &self,
        path: &str,
        content: &str,
        mime_type: Option<&str>,
    ) -> Result<()> {
        let mime = mime_type.unwrap_or(DEFAULT_TEXT_MIME);
        self.write_inner(path, content.as_bytes(), mime)
    }

    async fn write_file_bytes(
        &self,
        path: &str,
        content: &[u8],
        mime_type: Option<&str>,
    ) -> Result<()> {
        let mime = mime_type.unwrap_or(DEFAULT_BYTES_MIME);
        self.write_inner(path, content, mime)
    }

    async fn append_file(&self, path: &str, content: &[u8]) -> Result<()> {
        let existing = self.read_file_bytes(path).await?.unwrap_or_default();
        let existing_mime = self.lstat(path).await?.map(|s| s.mime_type);
        let mut combined = Vec::with_capacity(existing.len() + content.len());
        combined.extend_from_slice(&existing);
        combined.extend_from_slice(content);
        self.write_file_bytes(path, &combined, existing_mime.as_deref())
            .await
    }

    async fn read_dir(&self, path: &str) -> Result<Option<Vec<String>>> {
        let Some(entries) = self.read_dir_with_file_types(path).await? else {
            return Ok(None);
        };
        Ok(Some(entries.into_iter().map(|e| e.name).collect()))
    }

    async fn read_dir_with_file_types(&self, path: &str) -> Result<Option<Vec<DirEntry>>> {
        let p = normalize_path(path)?;
        let inner = self.lock();
        let Some(resolved) = Self::resolve_symlinks(&inner, &p, 0)? else {
            return Ok(None);
        };
        match inner.nodes.get(&resolved) {
            Some(Node::Directory { .. }) => {}
            _ => return Ok(None),
        }
        let prefix = if resolved == "/" {
            "/".to_string()
        } else {
            format!("{resolved}/")
        };
        let mut out: Vec<DirEntry> = inner
            .nodes
            .iter()
            .filter_map(|(k, v)| {
                let rest = k.strip_prefix(&prefix)?;
                if rest.is_empty() || rest.contains('/') {
                    None
                } else {
                    Some(DirEntry {
                        name: rest.to_string(),
                        kind: v.kind(),
                    })
                }
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(Some(out))
    }

    async fn mkdir(&self, path: &str, opts: MkdirOptions) -> Result<()> {
        let p = normalize_path(path)?;
        if p == "/" {
            return Ok(());
        }
        let mut emits: Vec<String> = Vec::new();
        {
            let mut inner = self.lock();
            if opts.recursive {
                let mut acc = String::new();
                for seg in p.split('/').filter(|s| !s.is_empty()) {
                    acc.push('/');
                    acc.push_str(seg);
                    if !inner.nodes.contains_key(&acc) {
                        inner.nodes.insert(
                            acc.clone(),
                            Node::Directory {
                                modified_at: now_secs(),
                            },
                        );
                        emits.push(acc.clone());
                    }
                }
            } else {
                let parent = parent_path(&p);
                if !parent.is_empty()
                    && parent != "/"
                    && !matches!(inner.nodes.get(&parent), Some(Node::Directory { .. }))
                {
                    return Err(FsError::NotFound(format!(
                        "mkdir parent {parent} does not exist"
                    )));
                }
                let existed = inner.nodes.contains_key(&p);
                if !existed {
                    inner.nodes.insert(
                        p.clone(),
                        Node::Directory {
                            modified_at: now_secs(),
                        },
                    );
                    emits.push(p.clone());
                }
            }
        }
        for e in emits {
            self.emit(WorkspaceChangeType::Create, &e, EntryType::Directory);
        }
        Ok(())
    }

    async fn rm(&self, path: &str, opts: RmOptions) -> Result<()> {
        let p = normalize_path(path)?;
        let to_delete: Vec<(String, EntryType)> = {
            let inner = self.lock();
            let Some(node) = inner.nodes.get(&p) else {
                if opts.force {
                    return Ok(());
                }
                return Err(FsError::NotFound(format!("rm {p} not found")));
            };
            match node.kind() {
                EntryType::File | EntryType::Symlink => vec![(p.clone(), node.kind())],
                EntryType::Directory => {
                    let prefix = if p == "/" {
                        "/".to_string()
                    } else {
                        format!("{p}/")
                    };
                    let has_children = inner
                        .nodes
                        .keys()
                        .any(|k| k != &p && k.starts_with(&prefix));
                    if has_children && !opts.recursive {
                        return Err(FsError::NotEmpty(format!(
                            "rm {p} is non-empty and recursive=false"
                        )));
                    }
                    let mut collected: Vec<(String, EntryType)> = inner
                        .nodes
                        .iter()
                        .filter(|(k, _)| *k == &p || k.starts_with(&prefix))
                        .map(|(k, v)| (k.clone(), v.kind()))
                        .collect();
                    // Children before parent so emit order matches upstream.
                    collected.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
                    collected
                }
            }
        };
        for (k, _) in &to_delete {
            self.lock().nodes.remove(k);
        }
        for (k, kind) in to_delete {
            self.emit(WorkspaceChangeType::Delete, &k, kind);
        }
        Ok(())
    }

    async fn cp(&self, src: &str, dst: &str, opts: CpOptions) -> Result<()> {
        let src = normalize_path(src)?;
        let dst = normalize_path(dst)?;
        let Some(src_stat) = self.lstat(&src).await? else {
            return Err(FsError::NotFound(format!(
                "no such file or directory: {src}"
            )));
        };
        match src_stat.kind {
            EntryType::File => {
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

    async fn mv(&self, src: &str, dst: &str) -> Result<()> {
        let src = normalize_path(src)?;
        let dst = normalize_path(dst)?;
        let Some(src_stat) = self.lstat(&src).await? else {
            return Err(FsError::NotFound(format!(
                "no such file or directory: {src}"
            )));
        };
        self.cp(
            &src,
            &dst,
            CpOptions {
                recursive: matches!(src_stat.kind, EntryType::Directory),
            },
        )
        .await?;
        self.rm(
            &src,
            RmOptions {
                recursive: matches!(src_stat.kind, EntryType::Directory),
                force: true,
            },
        )
        .await
    }

    async fn symlink(&self, target: &str, link_path: &str) -> Result<()> {
        if target.len() > MAX_PATH_LENGTH {
            return Err(FsError::NameTooLong(format!(
                "symlink target length {} exceeds {MAX_PATH_LENGTH}",
                target.len()
            )));
        }
        let p = normalize_path(link_path)?;
        {
            let mut inner = self.lock();
            Self::ensure_parent_dirs(&mut inner, &p)?;
            inner.nodes.insert(
                p.clone(),
                Node::Symlink {
                    target: target.to_string(),
                    modified_at: now_secs(),
                },
            );
        }
        self.emit(WorkspaceChangeType::Create, &p, EntryType::Symlink);
        Ok(())
    }

    async fn readlink(&self, path: &str) -> Result<Option<String>> {
        let p = normalize_path(path)?;
        Ok(self.lock().nodes.get(&p).and_then(|n| match n {
            Node::Symlink { target, .. } => Some(target.clone()),
            _ => None,
        }))
    }

    async fn realpath(&self, path: &str) -> Result<Option<String>> {
        let p = normalize_path(path)?;
        Self::resolve_symlinks(&self.lock(), &p, 0)
    }

    async fn glob(&self, pattern: &str) -> Result<Vec<String>> {
        let re = glob_to_regex(pattern);
        let mut out: Vec<String> = self
            .lock()
            .nodes
            .keys()
            .filter(|k| re.matches(k))
            .cloned()
            .collect();
        out.sort();
        Ok(out)
    }
}

fn make_stat(node: &Node) -> Stat {
    Stat {
        kind: node.kind(),
        size: node.size(),
        modified_at: node.modified_at(),
        mime_type: node.mime_type().to_string(),
        mode: Stat::mode_for(node.kind()),
    }
}

fn kind_name(k: EntryType) -> &'static str {
    match k {
        EntryType::File => "file",
        EntryType::Directory => "directory",
        EntryType::Symlink => "symlink",
    }
}

struct GlobMatcher {
    segments: Vec<GlobSeg>,
}

enum GlobSeg {
    StarStar,
    Chars(Vec<GlobChar>),
}

enum GlobChar {
    Star,
    Question,
    Literal(u8),
}

impl GlobMatcher {
    fn matches(&self, path: &str) -> bool {
        let path = path.strip_prefix('/').unwrap_or(path);
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        Self::match_segs(&self.segments, &segs)
    }

    fn match_segs(pat: &[GlobSeg], segs: &[&str]) -> bool {
        match pat.first() {
            None => segs.is_empty(),
            Some(GlobSeg::StarStar) => {
                for i in 0..=segs.len() {
                    if Self::match_segs(&pat[1..], &segs[i..]) {
                        return true;
                    }
                }
                false
            }
            Some(GlobSeg::Chars(chars)) => {
                let Some(seg) = segs.first() else { return false };
                if !match_chars(chars, seg.as_bytes()) {
                    return false;
                }
                Self::match_segs(&pat[1..], &segs[1..])
            }
        }
    }
}

fn match_chars(pat: &[GlobChar], s: &[u8]) -> bool {
    match pat.first() {
        None => s.is_empty(),
        Some(GlobChar::Star) => {
            for i in 0..=s.len() {
                if match_chars(&pat[1..], &s[i..]) {
                    return true;
                }
            }
            false
        }
        Some(GlobChar::Question) => {
            if s.is_empty() {
                false
            } else {
                match_chars(&pat[1..], &s[1..])
            }
        }
        Some(GlobChar::Literal(b)) => match s.first() {
            Some(c) if c == b => match_chars(&pat[1..], &s[1..]),
            _ => false,
        },
    }
}

fn glob_to_regex(pattern: &str) -> GlobMatcher {
    let pattern = pattern.strip_prefix('/').unwrap_or(pattern);
    let segments = pattern
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|seg| {
            if seg == "**" {
                GlobSeg::StarStar
            } else {
                let mut out: Vec<GlobChar> = Vec::new();
                for b in seg.bytes() {
                    match b {
                        b'*' => out.push(GlobChar::Star),
                        b'?' => out.push(GlobChar::Question),
                        c => out.push(GlobChar::Literal(c)),
                    }
                }
                GlobSeg::Chars(out)
            }
        })
        .collect();
    GlobMatcher { segments }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::conformance;

    // The unit tests below call the GENERIC conformance suite in
    // `crate::shell::conformance`. Anything that's a property of the
    // `FileSystem` contract (rather than InMemoryFs-specific behavior)
    // belongs in `conformance.rs` so Workspace can run the same checks.

    fn fs() -> InMemoryFs {
        InMemoryFs::new()
    }

    #[tokio::test]
    async fn round_trip() {
        conformance::round_trip(&fs()).await;
    }

    #[tokio::test]
    async fn enoent_returns_ok_none() {
        conformance::enoent_returns_ok_none(&fs()).await;
    }

    #[tokio::test]
    async fn eisdir_on_read_of_directory() {
        conformance::eisdir_on_read_of_directory(&fs()).await;
    }

    #[tokio::test]
    async fn eisdir_on_write_to_root() {
        conformance::eisdir_on_write_to_root(&fs()).await;
    }

    #[tokio::test]
    async fn name_too_long() {
        conformance::name_too_long(&fs()).await;
    }

    #[tokio::test]
    async fn rm_recursive() {
        conformance::rm_recursive(&fs()).await;
    }

    #[tokio::test]
    async fn cp_preserves_mime() {
        conformance::cp_preserves_mime(&fs()).await;
    }

    #[tokio::test]
    async fn on_change_emits_create_then_update_then_delete() {
        conformance::on_change_emits_create_then_update_then_delete(&fs()).await;
    }
}
