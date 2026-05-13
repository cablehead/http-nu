# cloudflare-shell-rpc-server

The Cloudflare Worker that exposes `cloudflare-shell`'s `FileSystem`
trait as a **Worker RPC binding**. See the parent crate's
[`README.md`](../README.md) for the overall story.

## What it exports

Six RPC methods on a `WorkerEntrypoint`, plus `fetch` as a health probe:

| Method        | Args              | Returns                  |
|---------------|-------------------|--------------------------|
| `readFile`    | `ReadFileReq`     | `ReadFileResp`           |
| `writeFile`   | `WriteFileReq`    | `Ack`                    |
| `stat`        | `StatReq`         | `StatResp`               |
| `mkdir`       | `MkdirReq`        | `Ack`                    |
| `rm`          | `RmReq`           | `Ack`                    |
| `list`        | `ListReq`         | `ListResp`               |
| `fetch`       | HTTP request      | `"shell-fs-rpc OK"`      |

The arg / return types are defined in
[`cloudflare-shell-rpc-types`](../types). Bytes travel base64-encoded.

## Architecture

```
RPC caller (JS or Rust) ── service binding ──> WorkerEntrypoint method
                                               (custom shim.js, sees this.env)
                                                └─> wasm export passed env + args
                                                     └─> get SHELL_FS_DO stub
                                                          └─> stub.fetch(internal req)
                                                               └─> ShellFsDo dispatches
                                                                    └─> Workspace::<method>
```

**Two extra hops vs. ideal RPC:** custom shim → wasm → DO fetch. Both
are forced by worker-rs 0.8 limitations (see rules below).

## Why a hand-written shim.js

worker-build's auto-generated shim wraps `fetch`/`queue`/`scheduled`
with `env` injection but leaves other `#[wasm_bindgen]` exports as
bare prototype assignments. Those bare exports don't see `this.env`
inside the wasm function. Until upstream
[wasm-bindgen#4757](https://github.com/rustwasm/wasm-bindgen/pull/4757)
lands, the workaround is a hand-written shim that owns the
`WorkerEntrypoint` class directly.

The shim lives at [`shim.js`](shim.js) and is what `wrangler.toml`
points to. The wasm bundle still gets built by `worker-build`; we
just bypass its shim and import the wbg exports directly.

## Why internal-fetch instead of typed DO methods

`#[durable_object]` in worker-rs 0.8 only exposes the `fetch(req)`
handler -- no typed RPC methods on the DO itself. Calls from the
entrypoint to the DO encode their args as JSON in a fake-URL fetch
(`POST /read_file`, etc.). The DO routes on `url.path()` internally.

When workers-rs grows typed DO RPC, the entrypoint <-> DO hop
collapses into a direct method call. Wire format and external API
don't change.

## Build / deploy

```bash
mise run cf:fs:build      # worker-build (release)
mise run cf:fs:dev        # wrangler dev (local; bound to port 8788)
mise run cf:fs:deploy     # wrangler deploy (needs CLOUDFLARE_API_TOKEN)
```

## Threat model + auth

The service binding lives **inside one Cloudflare account**: only
Workers deployed to the same account can bind to it. That gives
account-level authentication for free. Within that boundary the
default trust model is:

- Any Worker on the account that adds `services = [{ binding =
  "SHELL_FS", service = "cloudflare-shell-rpc" }]` to its wrangler.toml
  can call every RPC method.
- All namespaces are equally accessible.
- File contents have no per-namespace ACLs; the operator is expected
  to use distinct namespaces per tenant if multi-tenant isolation is
  needed.

For deployments where that's too permissive, the server supports an
**opt-in shared-secret token** check:

1. Set `SHELL_FS_TOKEN` on the server worker (via `wrangler secret put
   SHELL_FS_TOKEN` for prod, or `[vars]` in wrangler.toml for dev).
2. Every RPC method now requires the request to include `auth: "<the
   token>"`. Otherwise the server returns `ENOENT: authentication
   required` (deliberately vague -- doesn't leak whether auth is on).
3. Binding consumers need the **same** token in their own env. The
   demos look for `SHELL_FS_TOKEN`; the Rust client crate exposes
   `ShellFsService::with_auth(token)`.

The check is a plain string compare (subtle::CtCompare isn't available
on wasm in worker-rs). Token strings are not user-controlled, so a
timing-oracle attack is not a credible threat in the default model.

**Out of scope today** (potential follow-ups):
- Per-binding namespace allowlists (binding consumer X can only touch
  namespaces matching pattern Y).
- Cloudflare Access in front (browser-fronted callers).
- Cap'n'Proto-side identity via the binding fabric (when worker-rs
  exposes it).

## License

MIT.
