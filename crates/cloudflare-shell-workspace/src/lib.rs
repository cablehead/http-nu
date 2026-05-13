//! DurableObject + R2 implementation of `cloudflare-shell`'s
//! `FileSystem` trait.
//!
//! `Workspace` is a per-user filesystem stored as one SQLite table
//! (`cf_workspace_<namespace>`) inside a DurableObject's storage,
//! with files larger than 1.5MB spilled to R2 keyed under
//! `${prefix}/${namespace}<path>`. Schema-compatible with
//! [`@cloudflare/shell@0.3.6`](https://www.npmjs.com/package/@cloudflare/shell) --
//! data written from either side is readable by the other.
//!
//! # When to use this
//!
//! - You're inside a Workers Rust project (`worker` crate).
//! - You want a filesystem with persistence + arbitrary file sizes.
//! - You want bidirectional interop with `@cloudflare/shell`-using
//!   JS Workers in the same account.
//!
//! # When to write your own impl instead
//!
//! - You don't need DO persistence (a request-scoped in-memory impl
//!   is enough).
//! - You're not on Workers.
//! - You want a different storage backend (KV-only, R2-only,
//!   Service-bound, etc.).
//!
//! In that case depend on [`cloudflare-shell`](https://docs.rs/cloudflare-shell)
//! directly and `impl FileSystem for YourBackend`.
//!
//! # File layout
//!
//! Filenames mirror upstream `@cloudflare/shell`:
//!
//! - `filesystem.rs` <- upstream `filesystem.ts` -- the `Workspace` impl
//! - `schema.rs` <- extracted from inline DDL in `filesystem.ts`'s init
//!
//! See `PORT_STATUS.md` (in the crate root) for the running coverage
//! ledger and method-by-method mapping back to upstream.

pub mod conformance_runner;
pub mod filesystem;
mod schema;

pub use conformance_runner::run_conformance;
pub use filesystem::Workspace;
pub use schema::DEFAULT_NAMESPACE;

// Re-export the shared FS types so call sites that only depend on
// this crate don't have to add a second `use cloudflare_shell::*`.
pub use cloudflare_shell::{
    CpOptions, DirEntry, EntryType, FileSystem, FsError, MkdirOptions, OnChange, Result, RmOptions,
    Stat, WorkspaceChangeEvent, WorkspaceChangeType,
};
