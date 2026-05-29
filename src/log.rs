// Minimal logging facade. Desktop re-exports from `logging` (full broadcast
// channel + human/jsonl formatters). Non-desktop (wasm) falls back to
// eprintln/println, which is enough to be picked up by the host runtime
// (Workers' console logger, browser devtools, etc.).

#[cfg(feature = "desktop")]
pub use crate::logging::{log_error, log_print};

#[cfg(not(feature = "desktop"))]
pub fn log_error(error: &str) {
    eprintln!("{error}");
}

#[cfg(not(feature = "desktop"))]
pub fn log_print(message: &str) {
    println!("{message}");
}
