# cloudflare-shell-rpc-client

Typed Rust client wrapper for the
[`cloudflare-shell-rpc`](../server/) Worker. Use it from any other
Rust Worker on your Cloudflare account: declare a service binding,
convert the `Fetcher` to a `ShellFsService`, call typed methods.

```toml
[dependencies]
cloudflare-shell-rpc-client = "0.1"
cloudflare-shell-rpc-types  = "0.1"   # re-exported types
worker = "0.8"
```

`wrangler.toml`:

```toml
services = [{ binding = "SHELL_FS", service = "cloudflare-shell-rpc" }]
```

`src/lib.rs`:

```rust
use cloudflare_shell_rpc_client::{ShellFs, ShellFsService};
use worker::*;

#[event(fetch)]
async fn fetch(_req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let fs: ShellFsService = env.service("SHELL_FS")?.into();

    fs.write_file("alice", "/hello.txt", b"hello", Some("text/plain")).await?;
    let bytes = fs.read_file("alice", "/hello.txt").await?;
    let stat  = fs.stat("alice", "/hello.txt").await?;

    Response::ok(format!("read {:?} bytes, stat: {:?}", bytes.map(|b| b.len()), stat))
}
```

## What you get

A single async trait, `ShellFs`, implemented by `ShellFsService`:

```rust
async fn read_file(&self, namespace: &str, path: &str)
    -> Result<Option<Vec<u8>>>;
async fn write_file(&self, namespace: &str, path: &str, data: &[u8],
                    mime_type: Option<&str>) -> Result<()>;
async fn stat(&self, namespace: &str, path: &str)
    -> Result<Option<Stat>>;
async fn mkdir(&self, namespace: &str, path: &str, recursive: bool)
    -> Result<()>;
async fn rm(&self, namespace: &str, path: &str, recursive: bool, force: bool)
    -> Result<()>;
async fn list(&self, namespace: &str, path: &str)
    -> Result<Option<Vec<DirEntry>>>;
```

`Option<T>` on the read side mirrors `cloudflare-shell`'s ENOENT
convention (`Ok(None)` = "doesn't exist", not an error). Other failures
(EISDIR, EEXIST, etc.) surface as `Err(worker::Error::RustError(...))`
with POSIX-prefixed messages -- caller can match on prefix.

## Internals

Hand-written `wasm_bindgen extern "C"` block (`src/sys.rs`) declares
the JS-side method signatures: each takes a `JsValue` (the serde-encoded
request), returns a `js_sys::Promise`. The typed methods on
`ShellFsService` (`src/service.rs`) serialize the request via
`serde-wasm-bindgen`, await the promise, deserialize the response.

Why hand-written instead of WIT codegen: worker-codegen's WIT path is
pre-alpha and only supports primitives. Hand-rolling lets us pass typed
serde structs across the boundary.

When upstream `wasm-bindgen` ships proper RPC type generation (tracked
in the `cloudflare-shell-rpc/server` README), this crate can be
auto-generated and the hand-written `sys.rs` deleted. Wire format
stays the same.

## See also

- [`cloudflare-shell-rpc-server`](../server/) -- the Worker this client targets.
- [`cloudflare-shell-rpc-types`](../types/) -- shared wire structs (re-exported).
- [`../demo-rust/`](../demo-rust/) -- a deployable Worker using this crate end-to-end.
