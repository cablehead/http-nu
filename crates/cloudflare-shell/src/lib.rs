//! Backend-agnostic Rust port of [`@cloudflare/shell`](https://www.npmjs.com/package/@cloudflare/shell):
//! the `FileSystem` trait, shared types, `FsError`, `path_utils`, and
//! a generic conformance suite.
//!
//! Pair this crate with an impl to get a working filesystem:
//!
//! - [`cloudflare-shell-workspace`](https://docs.rs/cloudflare-shell-workspace)
//!   provides `Workspace`, backed by a DurableObject's SQLite storage
//!   plus R2 for files over 1.5MB. Schema-compatible with
//!   `@cloudflare/shell` (data written from either side is readable
//!   by the other).
//! - Or write your own impl of the `FileSystem` trait.
//!
//! This crate has no impls of its own. The split keeps the trait +
//! types reusable from any Workers (or non-Workers) project without
//! pulling in DO / R2 dependencies.
//!
//! # Features
//!
//! - `workers` (off by default) -- enables `From<worker::Error>` /
//!   `From<FsError> for worker::Error` conversions. Use this when
//!   writing a `FileSystem` impl that bridges `worker::SqlStorage`,
//!   `worker::Bucket`, or other Workers error boundaries.
//!
//! # Conformance
//!
//! [`conformance`] holds a generic `<F: FileSystem>` test suite.
//! Drive it against your impl in a wasm-target integration test (the
//! `cloudflare-shell-workspace` crate's harness is the reference
//! pattern).

pub mod conformance;
pub mod error;
pub mod interface;
pub mod path_utils;

pub use error::{FsError, Result};
pub use interface::{
    CpOptions, DirEntry, EntryType, FileSystem, MkdirOptions, OnChange, RmOptions, Stat,
    WorkspaceChangeEvent, WorkspaceChangeType, DEFAULT_BYTES_MIME, DEFAULT_DIR_MODE,
    DEFAULT_FILE_MODE, DEFAULT_TEXT_MIME, MAX_PATH_LENGTH, MAX_SYMLINK_DEPTH, SYMLINK_MODE,
};
