//! Wire types for the `cloudflare-shell-rpc` Worker.
//!
//! Pure Rust -- no `worker` dependency. The same structs are used by
//! the server (`cloudflare-shell-rpc`, wasm-only) and the Rust client
//! wrapper (`cloudflare-shell-rpc-client`, wasm-only). JS consumers
//! pass equivalent JSON objects.
//!
//! ## Bytes
//!
//! File contents travel as base64-encoded strings. Keeps the JSON
//! shape JS-friendly (avoids `Uint8Array` round-trip pain at the
//! service-binding boundary) at the cost of ~33% size overhead. The
//! base64 encode/decode happens on each side; this crate doesn't pull
//! `base64` itself, callers do.
//!
//! ## ENOENT semantics
//!
//! Mirrors `cloudflare-shell`: read-side responses use `Option<T>`
//! (`None` = ENOENT). Other errors come back as `RpcError`.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

// ── Shared sub-types ──────────────────────────────────────────────────

/// Mirror of `cloudflare_shell::EntryType`. Re-defined here (instead
/// of depending on `cloudflare-shell`) so this crate stays pure-Rust /
/// `worker`-free and so the wire format is decoupled from upstream
/// refactors of the FileSystem trait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryType {
    File,
    Directory,
    Symlink,
}

/// Mirror of `cloudflare_shell::Stat`. Wire shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stat {
    pub kind: EntryType,
    pub size: u64,
    #[serde(rename = "modifiedAt")]
    pub modified_at: i64,
    #[serde(rename = "mimeType")]
    pub mime_type: String,
    pub mode: u32,
}

/// Mirror of `cloudflare_shell::DirEntry`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    pub name: String,
    pub kind: EntryType,
}

/// RPC-side error. Mirrors `cloudflare_shell::FsError`'s discriminants
/// without pulling that crate in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", content = "message")]
pub enum RpcError {
    /// ENOENT. Note: most read-side methods surface ENOENT as
    /// `data: None` instead; this variant is for cases where ENOENT
    /// has to be an error (e.g. `rm` without `force`).
    NotFound(String),
    IsDir(String),
    NotDir(String),
    AlreadyExists(String),
    NotEmpty(String),
    NameTooLong(String),
    SymlinkLoop(String),
    InvalidUtf8(String),
    NoSpace(String),
    Io(String),
    /// Anything that doesn't map to one of the POSIX kinds above.
    Other(String),
}

// ── Request / response shapes ─────────────────────────────────────────
//
// Convention:
//   <Method>Req     -- argument struct (what the caller sends)
//   <Method>Resp    -- response struct (what the server returns on Ok)
//
// All requests carry:
//   - `namespace` -- one DO instance per namespace; the server routes
//     to the matching `Workspace`. Empty string maps to the default
//     namespace.
//   - `auth` (optional) -- if the server has `SHELL_FS_TOKEN` set as
//     an env var, every request must include `auth: Some(<token>)`
//     matching it. If the server has no token configured, the field
//     is ignored. See server/README.md for the threat model.
//
// All responses are wrapped at the wire-call site in
// `Result<Resp, RpcError>`; we don't bake the Result into the struct
// because serde-wasm-bindgen handles `Result` poorly across the JS
// boundary -- the server throws a JS Error on `Err`, the client
// reconstructs it.

/// `read_file(namespace, path)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadFileReq {
    pub namespace: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// Response for `read_file`. `data` is `None` on ENOENT, `Some(b64)`
/// otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadFileResp {
    /// Base64-encoded file contents, or `None` if the file doesn't
    /// exist.
    pub data: Option<String>,
}

/// `write_file(namespace, path, data, mime_type?)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WriteFileReq {
    pub namespace: String,
    pub path: String,
    /// Base64-encoded file contents.
    pub data: String,
    #[serde(rename = "mimeType", skip_serializing_if = "Option::is_none", default)]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// `stat(namespace, path)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatReq {
    pub namespace: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// Response for `stat`. `None` on ENOENT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatResp {
    pub stat: Option<Stat>,
}

/// `mkdir(namespace, path, recursive)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MkdirReq {
    pub namespace: String,
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// `rm(namespace, path, recursive, force)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RmReq {
    pub namespace: String,
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default)]
    pub force: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// `list(namespace, path)`. Maps to `read_dir_with_file_types`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListReq {
    pub namespace: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// Response for `list`. `entries` is `None` on ENOENT or when the
/// path is not a directory; `Some(vec![])` is an empty directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListResp {
    pub entries: Option<Vec<DirEntry>>,
}

/// `exists(namespace, path)`. Cheap presence probe -- does not follow
/// symlinks. Returns a plain `bool`; never `Ok(None)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistsReq {
    pub namespace: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExistsResp {
    pub exists: bool,
}

/// `lstat(namespace, path)`. Same shape as `StatReq`/`StatResp` but
/// does NOT follow the final symlink (matches Workspace::lstat).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LstatReq {
    pub namespace: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LstatResp {
    pub stat: Option<Stat>,
}

/// `append_file(namespace, path, data)`. Appends `data` (base64) onto
/// `path`; preserves the existing entry's mime_type. Errors if `path`
/// is a directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppendFileReq {
    pub namespace: String,
    pub path: String,
    /// Base64-encoded bytes to append.
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// `cp(namespace, src, dst, recursive)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpReq {
    pub namespace: String,
    pub src: String,
    pub dst: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// `mv(namespace, src, dst)`. Always behaves like `rename`; for
/// directory targets see Workspace::mv semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MvReq {
    pub namespace: String,
    pub src: String,
    pub dst: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// `symlink(namespace, target, link_path)`. Creates `link_path`
/// pointing at `target`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymlinkReq {
    pub namespace: String,
    pub target: String,
    #[serde(rename = "linkPath")]
    pub link_path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// `readlink(namespace, path)`. Returns the target string if `path`
/// is a symlink; `Ok(None)` if `path` does not exist or is not a
/// symlink.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadlinkReq {
    pub namespace: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadlinkResp {
    pub target: Option<String>,
}

/// `realpath(namespace, path)`. Resolves symlinks; returns the
/// canonical path, or `Ok(None)` on ENOENT.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealpathReq {
    pub namespace: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RealpathResp {
    pub path: Option<String>,
}

/// `glob(namespace, pattern)`. Returns absolute paths matching the
/// glob, sorted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobReq {
    pub namespace: String,
    pub pattern: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobResp {
    pub paths: Vec<String>,
}

/// `file_exists(namespace, path)`. Symlink-resolving; true only for
/// files (not directories or symlinks-to-directories).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileExistsReq {
    pub namespace: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileExistsResp {
    pub exists: bool,
}

/// `delete_file(namespace, path)`. File/symlink only; errors with
/// `IsDir` on a directory (use `rm` recursive instead). Returns
/// `removed: false` on ENOENT, `true` after a successful delete.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteFileReq {
    pub namespace: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteFileResp {
    pub removed: bool,
}

/// `get_workspace_info(namespace)`. No path -- aggregates across the
/// whole namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfoReq {
    pub namespace: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub auth: Option<String>,
}

/// Mirror of `cloudflare_shell_workspace::WorkspaceInfo`. Wire shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct WorkspaceInfo {
    #[serde(rename = "fileCount")]
    pub file_count: u64,
    #[serde(rename = "directoryCount")]
    pub directory_count: u64,
    #[serde(rename = "totalBytes")]
    pub total_bytes: u64,
    #[serde(rename = "r2FileCount")]
    pub r2_file_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceInfoResp {
    pub info: WorkspaceInfo,
}

/// Generic ack -- mutation methods return this on Ok.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Ack {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_file_resp_json_shape() {
        let some = ReadFileResp {
            data: Some("aGVsbG8=".into()),
        };
        let s = serde_json::to_string(&some).unwrap();
        assert_eq!(s, r#"{"data":"aGVsbG8="}"#);

        let none = ReadFileResp { data: None };
        let s = serde_json::to_string(&none).unwrap();
        assert_eq!(s, r#"{"data":null}"#);
    }

    #[test]
    fn write_file_req_camel_case() {
        let req = WriteFileReq {
            namespace: "alice".into(),
            path: "/x.txt".into(),
            data: "Zm9v".into(),
            mime_type: Some("text/plain".into()),
            auth: None,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains(r#""mimeType":"text/plain""#));
    }

    #[test]
    fn write_file_req_omits_mime_when_none() {
        let req = WriteFileReq {
            namespace: "".into(),
            path: "/x".into(),
            data: "".into(),
            mime_type: None,
            auth: None,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(!s.contains("mimeType"));
    }

    #[test]
    fn entry_type_lowercase() {
        assert_eq!(
            serde_json::to_string(&EntryType::File).unwrap(),
            r#""file""#
        );
        assert_eq!(
            serde_json::to_string(&EntryType::Directory).unwrap(),
            r#""directory""#
        );
        assert_eq!(
            serde_json::to_string(&EntryType::Symlink).unwrap(),
            r#""symlink""#
        );
    }

    #[test]
    fn rpc_error_tagged_shape() {
        let e = RpcError::NotFound("/missing".into());
        let s = serde_json::to_string(&e).unwrap();
        assert_eq!(s, r#"{"code":"NotFound","message":"/missing"}"#);
    }

    #[test]
    fn rm_req_defaults() {
        let json = r#"{"namespace":"","path":"/x"}"#;
        let r: RmReq = serde_json::from_str(json).unwrap();
        assert!(!r.recursive);
        assert!(!r.force);
    }
}
