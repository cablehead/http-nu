# cloudflare-shell-rpc

A Cloudflare Worker that exposes the `cloudflare-shell` `FileSystem`
trait (backed by `cloudflare-shell-workspace`'s DO SQLite + R2 impl)
as a **Worker RPC binding**. Any other Worker on your Cloudflare
account, JS or Rust, can bind to it as a service and call FS methods
directly -- no HTTP round-trip.

Independent of `http-nu`. Boots its own DurableObject class + its own
R2 bucket.

## Layout

```
types/         Wire types -- pure Rust, no `worker` dep. serde structs
               shared by server + client. Compiles on desktop too;
               unit-test serialization without a wasm toolchain.
server/        The Worker. wasm-only. `#[wasm_bindgen]` async methods
               exported as RPC. Routes each call to a `SHELL_FS_DO`
               stub keyed by namespace; the DO holds a `Workspace`.
client/        Typed Rust wrapper for binding consumers. Hand-written
               wasm-bindgen extern + async-trait. Depends on `types`.
demo-js/       Plain JS Worker (wrangler.toml + index.js). Proves the
               JS consumer story and provides curl-able HTTP routes
               for the smoke test.
demo-rust/     wasm Rust Worker. Depends on `client`. Proves the Rust
               consumer story and serves as the integration test for
               the `client` crate.
smoke/         End-to-end smoke test (`run.nu`). Drives the demo's
               curl-able HTTP surface; verifies round-trip + the
               bad-namespace rejection path. Run via `cf:fs:smoke{,:rust,:all}`.
bench/         oha-driven benchmark for the subsystem (mirrors
               benchmarks/bench-cf/). `run.nu` is single-URL; `matrix.nu`
               iterates the JS-vs-Rust grid. Run via `cf:fs:bench:all`.
pitchfork.toml Daemon definitions used by `cf:fs:up` / `cf:fs:down` /
               `cf:fs:smoke:all` to bring up + tear down all three
               Workers together with readiness probes + dep ordering.
DECISIONS.md   Durable design rationale (custom shim, base64 wire,
               internal-fetch DO dispatch, namespace validation,
               opt-in token auth, ...). Read before changing wire
               format or auth surfaces.
```

`server/` and the two demos are deployable Workers (each with its own
`wrangler.toml`). `types/` and `client/` are library crates.

## Consumer cheat sheet

**JS Worker** -- declare the service binding in `wrangler.toml`:

```toml
services = [{ binding = "SHELL_FS", service = "cloudflare-shell-rpc" }]
```

Then call methods directly: `await env.SHELL_FS.readFile({ namespace, path })`.
See `demo-js/index.js`.

**Rust Worker** -- add the crate + the same wrangler binding:

```toml
[dependencies]
cloudflare-shell-rpc-client = { version = "0.1" }
cloudflare-shell-rpc-types  = { version = "0.1" }
```

```rust
use cloudflare_shell_rpc_client::ShellFs;
let fs: cloudflare_shell_rpc_client::ShellFsService = env.service("SHELL_FS")?.into();
let bytes = fs.read_file("alice", "/notes.md").await?;
```

See `demo-rust/src/lib.rs`.

## Wire format

The shared serde structs are in `types/`. The wire is JSON-shaped
JS objects across the Worker RPC boundary (Cap'N Proto under the
hood; both sides use serde-wasm-bindgen / JSON respectively). Bytes
go base64-encoded so the JSON shape stays JS-friendly.

## Status

Bootstrapping. See the parent repo's
[`CLOUDFLARE_STATUS.md`](../../CLOUDFLARE_STATUS.md) for the running
state of CF work.

## Live demo

The subsystem is deployed on Cloudflare Workers:

| Worker | URL |
|---|---|
| `cloudflare-shell-rpc` (the RPC server) | <https://cloudflare-shell-rpc.gedw99.workers.dev> |
| `cloudflare-shell-rpc-demo-js` (JS consumer) | <https://cloudflare-shell-rpc-demo-js.gedw99.workers.dev> |
| `cloudflare-shell-rpc-demo-rust` (Rust consumer via the client crate) | <https://cloudflare-shell-rpc-demo-rust.gedw99.workers.dev> |

The server enforces token auth (`SHELL_FS_TOKEN`); demos handle it
internally via their own secret, so the demo URLs are usable without
a token. The server URL requires `Authorization: Bearer <token>` on
every FS route -- root `/` is always open as a banner.

Try them with curl:

```bash
# JS demo -- write, then read, then stat
curl -X PUT --data hello https://cloudflare-shell-rpc-demo-js.gedw99.workers.dev/fs/alice/note.txt
curl https://cloudflare-shell-rpc-demo-js.gedw99.workers.dev/fs/alice/note.txt
curl https://cloudflare-shell-rpc-demo-js.gedw99.workers.dev/stat/alice/note.txt
```

Full lifecycle: `mise run cf:fs:deploy:all` (deploy + push secrets) /
`mise run cf:fs:smoke:remote` (verify) / `mise run cf:fs:teardown`
(destroy).

## Benchmark report

Latest measured throughput / latency across the three tiers (server
direct, JS demo via RPC binding, Rust demo via RPC binding + typed
client) is published at [`bench/REPORT.md`](bench/REPORT.md), with
both **local-dev** (`wrangler dev`) and **remote-edge** (deployed
Workers) rows side-by-side.

For interpretation rather than raw numbers, jump straight to
[`bench/REPORT.md#analysis`](bench/REPORT.md#analysis) -- it
auto-computes headline takeaways (dev-vs-edge gap, binding + RPC
overhead, typed Rust client cost) and explains what the numbers do
and don't tell you. Both the tables and the analysis prose are
regenerated by `cf:fs:bench:report`; **don't hand-edit `REPORT.md`**.

Regenerate:
- Local: `mise run cf:fs:bench:all`
- Remote: `mise run cf:fs:bench:remote && mise run cf:fs:bench:report`

## Running everything together

The three Workers (server + two demos) need each other up to be
useful (the demos resolve their `SHELL_FS` service binding via
wrangler's local dev registry, which requires the server to be
running first). `pitchfork.toml` codifies the startup order +
readiness probes; one command brings everything up:

```bash
mise run cf:fs:up          # start all three (waits for HTTP ready)
mise run cf:fs:status      # see daemon states
mise run cf:fs:logs WHICH=demo-js   # tail one (server | demo-js | demo-rust)
mise run cf:fs:down        # stop all three

mise run cf:fs:smoke:all   # bring up, smoke both demos, bring down
```

Pitchfork is pinned to v2.10.0 from `github:endevco/pitchfork`.
