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

    #[wasm_bindgen(method, catch, js_name = "exists")]
    pub fn exists(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "lstat")]
    pub fn lstat(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "appendFile")]
    pub fn append_file(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "cp")]
    pub fn cp(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "mv")]
    pub fn mv(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "symlink")]
    pub fn symlink(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "readlink")]
    pub fn readlink(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "realpath")]
    pub fn realpath(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "glob")]
    pub fn glob(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "fileExists")]
    pub fn file_exists(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "deleteFile")]
    pub fn delete_file(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(method, catch, js_name = "workspaceInfo")]
    pub fn workspace_info(this: &ShellFsSys, args: JsValue) -> Result<Promise, JsValue>;
}
