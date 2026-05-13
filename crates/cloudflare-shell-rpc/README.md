# cloudflare-shell-rpc

> One of three crates in the `cloudflare-shell` family. For the
> family overview + dependency graph, see [`../README.md`](../README.md).

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
demo-js/       JS Worker (wrangler.toml + index.js). Two faces:
               GET / serves an interactive file-browser UI (vanilla
               HTML/CSS/JS, ~470 lines, no build step) with tree view,
               file viewer (text/JSON/hex), drag-drop upload,
               namespace switcher. Other routes are curl-able HTTP for
               the smoke test + JS-consumer reference.
demo-rust/     wasm Rust Worker. Depends on `client`. Mirrors demo-js's
               curl-able HTTP routes (no UI) so the smoke + bench can
               run identically against both consumers; that's how we
               isolate the typed Rust client wrapper's cost.
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
use cloudflare_shell_rpc_client::{ShellFs, ShellFsService};

// no-auth (server has no SHELL_FS_TOKEN set):
let fs: ShellFsService = env.service("SHELL_FS")?.into();

// with auth (server has SHELL_FS_TOKEN set; consumer needs a matching secret):
let fs: ShellFsService = env.service("SHELL_FS")?.into().with_auth(env.secret("SHELL_FS_TOKEN")?.to_string());

let bytes = fs.read_file("alice", "/notes.md").await?;
```

See `demo-rust/src/lib.rs`.

## Wire format

The shared serde structs are in `types/`. The wire is JSON-shaped
JS objects across the Worker RPC boundary (Cap'N Proto under the
hood; both sides use serde-wasm-bindgen / JSON respectively). Bytes
go base64-encoded so the JSON shape stays JS-friendly.

## Live demo

The subsystem is deployed on Cloudflare Workers. **Click the JS demo
link first** -- it's an interactive file-browser UI built on the
same routes the other demos expose:

| Worker | URL | What you get in a browser |
|---|---|---|
| `cloudflare-shell-rpc-demo-js` (JS consumer + UI) | <https://cloudflare-shell-rpc-demo-js.gedw99.workers.dev> | Interactive file-browser UI: tree view, file viewer (text / JSON / hex), drag-drop upload, namespace switcher, mkdir + delete inline. Vanilla JS, no build step. |
| `cloudflare-shell-rpc-demo-rust` (Rust consumer via the client crate) | <https://cloudflare-shell-rpc-demo-rust.gedw99.workers.dev> | Plain HTTP banner + JSON routes (no UI). Sibling to demo-js for benching the typed Rust client wrapper. |
| `cloudflare-shell-rpc` (the RPC server) | <https://cloudflare-shell-rpc.gedw99.workers.dev> | Plain banner; FS routes require `Authorization: Bearer <token>`. |

**Try it now:** open the JS demo link, type any namespace name
matching `[a-zA-Z][a-zA-Z0-9_]*`, drag a file onto the page. The
file lands in a DurableObject SQLite row (or R2 if > 1.5MB). Switch
to another namespace and it's isolated. Try `bad-ns` (hyphen) to see
the server-side validator reject it.

For curl users:

```bash
# JS demo -- write, then read, then stat
curl -X PUT --data hello https://cloudflare-shell-rpc-demo-js.gedw99.workers.dev/fs/alice/note.txt
curl https://cloudflare-shell-rpc-demo-js.gedw99.workers.dev/fs/alice/note.txt
curl https://cloudflare-shell-rpc-demo-js.gedw99.workers.dev/stat/alice/note.txt
```

Auth: server enforces `SHELL_FS_TOKEN` (sent as `Authorization:
Bearer <token>` on HTTP, `auth:` field on RPC). Demos thread the
token internally via their own secret, so demo URLs are open.

Full lifecycle: `mise run cf:fs:deploy:all` (deploy + push secrets) /
`mise run cf:fs:smoke:remote` (verify) / `mise run cf:fs:teardown`
(destroy).

## Benchmark report

Latest measured throughput / latency across the three tiers (server
direct, JS demo via RPC binding, Rust demo via RPC binding + typed
client) is published as two separate files:

- [**`bench/REPORT.remote.md`**](bench/REPORT.remote.md) -- production
  rows. Deployed Workers on the real CF edge. Quote these.
- [**`bench/REPORT.local.md`**](bench/REPORT.local.md) -- `wrangler
  dev` rows for regression spotting / JS-vs-Rust relative deltas.
  **Not** production numbers.

Both files embed the same Deployment sizes table from `sizes.nuon`.

For interpretation rather than raw numbers, the **Analysis** section in
each report auto-computes headline takeaways (binding + RPC overhead,
typed Rust client cost) and explains what the numbers do and don't tell
you. Tables and analysis prose are regenerated by `cf:fs:bench:report`;
**don't hand-edit either `REPORT.*.md`**.

Regenerate:
- Local: `mise run cf:fs:bench:all`
- Remote: `SHELL_FS_TOKEN="$(fnox get SHELL_FS_TOKEN)" mise run cf:fs:bench:remote && mise run cf:fs:bench:report`

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

mise run cf:fs:smoke:all   # bring up, smoke all three tiers, bring down
```

Pitchfork is pinned to v2.10.0 from `github:endevco/pitchfork`.
