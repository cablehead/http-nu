//! Hand-written `wasm_bindgen extern "C"` block matching the
//! JS-side WorkerEntrypoint methods exported by
//! `cloudflare-shell-rpc-server`'s `shim.js`.
//!
//! Each method takes a `JsValue` (a `serde-wasm-bindgen`-encoded
//! request struct) and returns a `js_sys::Promise` resolving to a
//! `JsValue` (the response struct).
//!
//! This file is mechanical / repetitive on purpose. When upstream
//! `wasm-bindgen` ships first-class RPC type generation we delete
//! this whole module.

use js_sys::Promise;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    /// Opaque JS-side handle to the WorkerEntrypoint on the other end
    /// of the service binding. Obtained via `worker::Fetcher::into_rpc()`.
    #[wasm_bindgen(extends = ::js_sys::Object)]
    pub type ShellFsSys;

    #[wasm_bindgen(method, catch, js_name = "readFile")]
    pub fn read_file(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "writeFile")]
    pub fn write_file(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "stat")]
    pub fn stat(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "mkdir")]
    pub fn mkdir(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "rm")]
    pub fn rm(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "list")]
    pub fn list(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;
}
