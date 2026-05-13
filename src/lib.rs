#![allow(clippy::result_large_err)]
pub mod bus;
pub mod commands;
pub mod compression;
pub mod engine;
pub mod log;
pub mod request;
pub mod response;
pub mod stdlib;

// Backend-agnostic FS abstraction (FileSystem trait, InMemoryFs, FsError,
// shared types). Reachable from BOTH desktop and wasm so InMemoryFs-backed
// unit tests run under `cargo test` and conformance tests can exercise the
// same code against either backend. The wasm-only Workspace impl lives
// under src/cf/shell/ and `impl crate::shell::FileSystem for Workspace`.
pub mod shell;

// Filesystem-call abstraction shared by desktop and wasm. `Vfs` trait +
// per-thread install/with hooks let upstream files do FS ops without
// cfg gates at the call site -- they call `crate::vfs::with_vfs(...)`
// and get `OsVfs` on desktop, `SnapshotVfs` on CF. See `src/vfs.rs`.
pub mod vfs;

// Cloudflare Workers entrypoint. Additive: lives in src/cf/, never imported
// on desktop, never modifies upstream files in this directory. Gated to
// wasm32 so a host-target build with `--all-features` (e.g. clippy) skips it.
#[cfg(all(feature = "cloudflare", target_arch = "wasm32"))]
pub mod cf;

// Modules that depend on desktop-only crates (hyper server, rustls, ctrlc,
// notify, tower-http/fs, std::thread). On Cloudflare these get a different
// implementation via worker-rs; they're not just stubbed out. Tracking the
// CF replacements is in CLOUDFLARE.md.
#[cfg(feature = "desktop")]
pub mod handler;
#[cfg(feature = "desktop")]
pub mod listener;
#[cfg(feature = "desktop")]
pub mod logging;
#[cfg(feature = "desktop")]
pub mod store;
#[cfg(feature = "desktop")]
pub mod worker;

#[cfg(all(test, feature = "desktop"))]
mod test_engine;
#[cfg(all(test, feature = "desktop"))]
mod test_handler;

pub use engine::Engine;
#[cfg(feature = "desktop")]
pub use listener::Listener;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
