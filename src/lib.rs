#![allow(clippy::result_large_err)]
pub mod bus;
pub mod commands;
pub mod compression;
pub mod engine;
pub mod log;
pub mod request;
pub mod response;
pub mod stdlib;

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
