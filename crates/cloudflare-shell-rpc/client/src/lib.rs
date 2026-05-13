//! Typed Rust client for the `cloudflare-shell-rpc` Worker.
//!
//! Two halves:
//!   - `sys` -- hand-written `extern "C"` block matching the
//!     WorkerEntrypoint method signatures exported by the server's
//!     `shim.js`. Pure JS-side typing.
//!   - `service` -- typed `ShellFs` async trait + `ShellFsService` impl
//!     that serialize/deserialize via `serde-wasm-bindgen`.
//!
//! See `README.md` for the user-facing usage and `crates/cloudflare-shell-rpc/server`
//! for what the wire looks like on the other side.

#![cfg(target_arch = "wasm32")]

mod service;
mod sys;

pub use service::{ShellFs, ShellFsService};

// Re-export the wire types so consumers don't need a second `use`
// statement. They get `Stat`, `DirEntry`, `EntryType`, `RpcError`
// straight from `cloudflare_shell_rpc_client::*`.
pub use cloudflare_shell_rpc_types::{DirEntry, EntryType, RpcError, Stat};
