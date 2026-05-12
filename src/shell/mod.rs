//! Backend-agnostic shell FS layer.
//!
//! This module sits at the top level (not under `src/cf/`) on purpose:
//! the `FileSystem` trait and the in-memory impl must be reachable from
//! desktop tests, not just from wasm. Wasm-specific implementations
//! (today: `crate::cf::shell::Workspace`) live under `src/cf/shell/`
//! and `impl crate::shell::FileSystem for ...`.
//!
//! Three docs sit next to this file:
//! - `README.md` -- orientation.
//! - `CLAUDE.md` -- contributor rules (provenance, conformance discipline,
//!                  the mock-divergence warning).
//!
//! See `src/cf/shell/PORT_STATUS.md` for the running upstream coverage
//! ledger.

pub mod conformance;
pub mod error;
pub mod in_memory_fs;
pub mod interface;
pub mod path_utils;

pub use error::{FsError, Result};
pub use in_memory_fs::InMemoryFs;
pub use interface::{
    CpOptions, DirEntry, EntryType, FileSystem, MkdirOptions, OnChange, RmOptions, Stat,
    WorkspaceChangeEvent, WorkspaceChangeType, DEFAULT_BYTES_MIME, DEFAULT_DIR_MODE,
    DEFAULT_FILE_MODE, DEFAULT_TEXT_MIME, MAX_PATH_LENGTH, MAX_SYMLINK_DEPTH, SYMLINK_MODE,
};
