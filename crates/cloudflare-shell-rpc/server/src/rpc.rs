//! Entrypoint-side RPC methods. The shim.js exports each of these as
//! a method on the `WorkerEntrypoint`, called by other Workers via
//! their service binding.
//!
//! Each function takes `(env, args)`:
//!   - `env`: the entrypoint's `this.env`, passed by the shim. We
//!     declare it as `worker::Env` directly (wasm-bindgen converts a
//!     JS object reference to `Env` via the type alias in worker-rs).
//!   - `args`: a `JsValue` of the matching wire request struct. We
//!     decode it via `serde-wasm-bindgen`.
//!
//! Each function:
//!   1. Decode args.
//!   2. Get the `ShellFsDo` namespace + stub for `args.namespace`.
//!   3. Build an internal fetch (`POST /<method>`) carrying the args
//!      as JSON.
//!   4. Decode the DO's JSON response back into the wire response.
//!   5. Encode the response into a `JsValue` for the caller.
//!
//! Errors raised here become JS Errors at the service-binding
//! boundary; the caller (JS or Rust) sees them as a thrown / `Err`.

use cloudflare_shell_rpc_types::{
    ListReq, ListResp, MkdirReq, ReadFileReq, ReadFileResp, RmReq, StatReq, StatResp, WriteFileReq,
};
use serde::{de::DeserializeOwned, Serialize};
use wasm_bindgen::prelude::*;
use worker::Env;

use crate::wire::{build_request, call_do};

const DO_BINDING: &str = "SHELL_FS_DO";

/// Env var name. If set, every RPC method requires `auth: Some(<value>)`
/// to match. If unset, no auth check runs. See `server/README.md` for
/// the threat model.
const TOKEN_ENV: &str = "SHELL_FS_TOKEN";

/// Decode args, validate auth, route to the namespace's DO stub,
/// decode response, re-encode as JsValue. Shared body for every RPC
/// method.
async fn dispatch<Req, Resp>(env: Env, args: JsValue, route: &str) -> Result<JsValue, JsValue>
where
    Req: DeserializeOwned + Serialize + AuthCarrier,
    Resp: DeserializeOwned + Serialize,
{
    let req: Req = serde_wasm_bindgen::from_value(args)
        .map_err(|e| JsValue::from_str(&format!("decode args: {e}")))?;

    // Auth: opt-in via SHELL_FS_TOKEN env var. Constant-time compare
    // is overkill here -- tokens are not user-controlled strings, and
    // worker-rs's runtime doesn't expose subtle::CtCompare to wasm --
    // but reject NotEqual + missing as the same error to avoid hinting
    // whether a server has auth enabled.
    if let Ok(token) = env.var(TOKEN_ENV).map(|v| v.to_string()) {
        let supplied = req.auth().unwrap_or("");
        if supplied != token {
            return Err(JsValue::from_str(
                "ENOENT: authentication required (set `auth` on the request)",
            ));
        }
    }

    let ns = env
        .durable_object(DO_BINDING)
        .map_err(|e| JsValue::from_str(&format!("get DO binding: {e}")))?;
    let id = ns
        .id_from_name(req.namespace())
        .map_err(|e| JsValue::from_str(&format!("DO id_from_name: {e}")))?;
    let stub = id
        .get_stub()
        .map_err(|e| JsValue::from_str(&format!("DO get_stub: {e}")))?;

    let internal_req = build_request(route, &req)
        .map_err(|e| JsValue::from_str(&format!("build internal request: {e}")))?;
    let resp: Resp = call_do(&stub, internal_req)
        .await
        .map_err(|e| JsValue::from_str(&e.to_string()))?;
    serde_wasm_bindgen::to_value(&resp)
        .map_err(|e| JsValue::from_str(&format!("encode response: {e}")))
}

/// Pulls `namespace` + optional `auth` out of any `*Req`, so `dispatch`
/// can do its routing + auth check without a fat match.
trait AuthCarrier {
    fn namespace(&self) -> &str;
    fn auth(&self) -> Option<&str>;
}

macro_rules! impl_auth_carrier {
    ($t:ty) => {
        impl AuthCarrier for $t {
            fn namespace(&self) -> &str {
                &self.namespace
            }
            fn auth(&self) -> Option<&str> {
                self.auth.as_deref()
            }
        }
    };
}
impl_auth_carrier!(ReadFileReq);
impl_auth_carrier!(WriteFileReq);
impl_auth_carrier!(StatReq);
impl_auth_carrier!(MkdirReq);
impl_auth_carrier!(RmReq);
impl_auth_carrier!(ListReq);

// ── #[wasm_bindgen] exports ───────────────────────────────────────────
//
// `js_name` keeps the JS-facing identifier in camelCase to match
// idiomatic Worker RPC method naming.

#[wasm_bindgen(js_name = readFile)]
pub async fn read_file(env: Env, args: JsValue) -> Result<JsValue, JsValue> {
    dispatch::<ReadFileReq, ReadFileResp>(env, args, "/read_file").await
}

#[wasm_bindgen(js_name = writeFile)]
pub async fn write_file(env: Env, args: JsValue) -> Result<JsValue, JsValue> {
    dispatch::<WriteFileReq, cloudflare_shell_rpc_types::Ack>(env, args, "/write_file").await
}

#[wasm_bindgen(js_name = stat)]
pub async fn stat(env: Env, args: JsValue) -> Result<JsValue, JsValue> {
    dispatch::<StatReq, StatResp>(env, args, "/stat").await
}

#[wasm_bindgen(js_name = mkdir)]
pub async fn mkdir(env: Env, args: JsValue) -> Result<JsValue, JsValue> {
    dispatch::<MkdirReq, cloudflare_shell_rpc_types::Ack>(env, args, "/mkdir").await
}

#[wasm_bindgen(js_name = rm)]
pub async fn rm(env: Env, args: JsValue) -> Result<JsValue, JsValue> {
    dispatch::<RmReq, cloudflare_shell_rpc_types::Ack>(env, args, "/rm").await
}

#[wasm_bindgen(js_name = list)]
pub async fn list(env: Env, args: JsValue) -> Result<JsValue, JsValue> {
    dispatch::<ListReq, ListResp>(env, args, "/list").await
}
