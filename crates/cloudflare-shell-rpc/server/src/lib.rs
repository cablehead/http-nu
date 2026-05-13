//! `cloudflare-shell-rpc-server` -- the Worker.
//!
//! Layout:
//!   lib.rs    `#[event(fetch)]` health endpoint + module wiring.
//!   rpc.rs    The `#[wasm_bindgen]` RPC methods. Called by the custom
//!             shim.js with `(this.env, args)`. Each method decodes
//!             args, gets a `ShellFsDo` stub by namespace, dispatches
//!             over an internal fetch, decodes the response.
//!   do.rs     The `ShellFsDo` Durable Object. Owns a `Workspace` (DO
//!             SQLite + R2). Routes incoming internal fetches on
//!             `url.path()` ("/read_file", "/write_file", ...).
//!   wire.rs   Shared helpers for (de)serializing wire types over the
//!             entrypoint <-> DO boundary.

mod do_obj;
mod http;
mod rpc;
mod wire;

use worker::{event, Context, Env, Request, Response, Result};

pub use do_obj::ShellFsDo;
// RPC method exports are pulled in by `rpc` being compiled; the
// `#[wasm_bindgen]` attribute makes them visible to the shim.
#[allow(unused_imports)]
use rpc::*;

#[event(fetch)]
async fn fetch(mut req: Request, env: Env, _ctx: Context) -> Result<Response> {
    console_error_panic_hook::set_once();
    let url = req.url()?;
    let path = url.path().to_string();
    if http::handles(&path) {
        return http::handle(&mut req, env).await;
    }
    Response::ok("shell-fs-rpc OK")
}
