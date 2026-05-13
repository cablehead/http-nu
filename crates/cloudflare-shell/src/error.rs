//! Crate-local error type for the shell FS surface. Replaces
//! `worker::Error::RustError("EISDIR: ...")` strings with typed variants
//! so callers can `matches!(err, FsError::IsDir(_))` instead of parsing
//! English prefixes.
//!
//! Display impls preserve the POSIX-style error-prefix convention used
//! upstream (`ENOENT:`, `EISDIR:`, etc.).
//!
//! With the `workers` feature on (wasm32 only), `From<worker::Error>`
//! converts boundary errors from `worker::SqlStorage` / `worker::Bucket`
//! into `Io`, and `From<FsError> for worker::Error` round-trips them
//! back through the worker fetch error path.

use std::fmt;

/// POSIX-style errno categories surfaced by `FileSystem` impls.
#[derive(Debug, Clone)]
pub enum FsError {
    /// ENOENT -- path does not exist.
    NotFound(String),
    /// EISDIR -- target is a directory when a file was expected (or
    /// vice versa for the EISDIR-on-write-to-root case).
    IsDir(String),
    /// ENOTDIR -- a path component is not a directory.
    NotDir(String),
    /// ENOTEMPTY -- directory is non-empty and recursive=false.
    NotEmpty(String),
    /// ENAMETOOLONG -- path or symlink target exceeds the configured max.
    NameTooLong(String),
    /// ELOOP -- symlink chain exceeds `MAX_SYMLINK_DEPTH`.
    SymlinkLoop(String),
    /// EILSEQ -- invalid byte sequence (e.g. non-utf8 readFile).
    InvalidEncoding(String),
    /// EIO -- underlying I/O / SQL / R2 failure.
    Io(String),
    /// ENOSPC -- write rejected because no R2 bucket is bound for spill.
    NoSpace(String),
    /// Anything else.
    Other(String),
}

impl fmt::Display for FsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(s) => write!(f, "ENOENT: {s}"),
            Self::IsDir(s) => write!(f, "EISDIR: {s}"),
            Self::NotDir(s) => write!(f, "ENOTDIR: {s}"),
            Self::NotEmpty(s) => write!(f, "ENOTEMPTY: {s}"),
            Self::NameTooLong(s) => write!(f, "ENAMETOOLONG: {s}"),
            Self::SymlinkLoop(s) => write!(f, "ELOOP: {s}"),
            Self::InvalidEncoding(s) => write!(f, "EILSEQ: {s}"),
            Self::Io(s) => write!(f, "EIO: {s}"),
            Self::NoSpace(s) => write!(f, "ENOSPC: {s}"),
            Self::Other(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for FsError {}

/// `Result` alias used across the shell surface.
pub type Result<T> = std::result::Result<T, FsError>;

#[cfg(feature = "workers")]
impl From<worker::Error> for FsError {
    fn from(e: worker::Error) -> Self {
        // Errors raised by SqlStorage / Bucket / R2 bubble up as `Io`.
        // Callers that need a finer category should wrap explicitly
        // before the `?`.
        FsError::Io(e.to_string())
    }
}

#[cfg(feature = "workers")]
impl From<FsError> for worker::Error {
    fn from(e: FsError) -> Self {
        // Surface to the worker fetch error path. The Display impl
        // includes the POSIX prefix.
        worker::Error::RustError(e.to_string())
    }
}
