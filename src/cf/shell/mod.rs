//! Wasm-only impls of the `@cloudflare/shell` Rust port.
//!
//! The trait + shared types (`FileSystem`, `Stat`, `EntryType`, `FsError`,
//! `InMemoryFs`, `path_utils`) live at top-level `crate::shell` so they
//! are reachable from desktop tests. Only the `Workspace` impl, which
//! depends on `worker::SqlStorage` + `worker::Bucket`, lives here.
//!
//! See:
//! - `crate::shell` -- the trait + InMemoryFs + shared types.
//! - `src/shell/CLAUDE.md` -- contributor rules + conformance discipline.
//! - `PORT_STATUS.md` (next to this file) -- upstream coverage ledger.
//!
//! Upstream `@cloudflare/shell` -> here / crate::shell:
//!   filesystem.ts         -> here::filesystem.rs       (Workspace, the wasm impl)
//!   fs/in-memory-fs.ts    -> crate::shell::in_memory_fs (InMemoryFs)
//!   fs/path-utils.ts      -> crate::shell::path_utils
//!   fs/interface.ts       -> crate::shell::interface    (FileSystem trait + types)
//!   (no upstream file)    -> here::schema.rs            (SQL DDL extracted from
//!                                                       filesystem.ts's init)
//!
//! Schema-compatible with the JS package -- same `cf_workspace_<ns>` table
//! layout, same R2 key shape (`${prefix}/${ns}<path>`), same 1.5MB
//! inline-vs-spill threshold. Data written from either side is readable
//! by the other.

pub mod filesystem;
mod schema;

pub use filesystem::Workspace;
pub use schema::DEFAULT_NAMESPACE;

// Re-export the shared FS types from the top-level abstraction so
// existing CF call sites can keep writing `shell::MkdirOptions` etc.
// New code SHOULD reach for `crate::shell::*` directly; this is a
// compatibility convenience, not a permanent API.
pub use crate::shell::{
    CpOptions, DirEntry, EntryType, FileSystem, FsError, MkdirOptions, OnChange, Result,
    RmOptions, Stat, WorkspaceChangeEvent, WorkspaceChangeType,
};
