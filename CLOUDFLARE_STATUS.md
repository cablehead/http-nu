# http-nu on Cloudflare Workers -- status

Running state: what works on the live worker today, which examples
are verified vs. blocked, and the orthogonal work tracks needed to
unblock the rest. The durable design narrative (merge story, xs
split, Vfs symmetry, handler lifecycle) lives in
[`CLOUDFLARE.md`](CLOUDFLARE.md).

**Live:** https://http-nu-cf.gedw99.workers.dev (serves
`examples/cf-workspace-browser`)

Per-subsystem ledgers, also running state:

- Nu shadow commands: [`src/cf/nu/nu_command/PORT_STATUS.md`](src/cf/nu/nu_command/PORT_STATUS.md)
- `@cloudflare/shell` Rust port: [`crates/cloudflare-shell-workspace/PORT_STATUS.md`](crates/cloudflare-shell-workspace/PORT_STATUS.md)

## What works on the live worker

- **Per-user routing** via the URL's first path segment: `/alice/...`
  lands in alice's DurableObject, `/bob/...` lands in bob's.
- **Per-user FS** backed by DO SQLite + R2 spill, via the
  `@cloudflare/shell` Rust port at
  [`crates/cloudflare-shell-workspace/`](crates/cloudflare-shell-workspace/README.md). R2 spill at 1.5MB
  (verified live with a 2MB file round-trip).
- **Nu shadow commands** read/write the per-request snapshot via the
  `Vfs` trait. Pending writes async-flush after eval. The current
  shadow set + reasons each one exists is tracked in
  [`src/cf/nu/nu_command/PORT_STATUS.md`](src/cf/nu/nu_command/PORT_STATUS.md).
- **`.static`** via the existing `RESPONSE_TX` pattern; serves from
  Workspace with Content-Type from extension.
- **Per-user handler hot-swap** via `PUT /<user>/admin/handler` (direct
  engine swap) OR via a Workspace write to `/serve.nu` -- the latter
  fires `onChange` on the user's Workspace, the next request notices
  the flag and re-parses through the cached engine. CF equivalent of
  desktop `--watch`, with Workspace as the transport.
- **Debug routes** `/<user>/_workspace/{ls,stat,cat,put,rm,mkdir,conformance}`.
  The `conformance` route runs `cloudflare_shell::conformance`'s
  generic `<F: FileSystem>` suite against the real `Workspace`. `200`
  + `<n> passed` body means every assertion holds; `500` + backtrace
  means the first assertion that failed. This is the only leg of the
  parity check (no desktop double); the route verifies the real DO
  SQLite + R2 backend matches the trait contract.

```bash
# read alice's workspace
curl https://http-nu-cf.gedw99.workers.dev/alice/_workspace/ls?path=/

# write a file
curl -X POST --data-binary @notes.md \
  https://http-nu-cf.gedw99.workers.dev/alice/file?path=/notes.md

# read it back through Nu's shadowed `open`
curl https://http-nu-cf.gedw99.workers.dev/alice/file?path=/notes.md

# upload a custom handler for this user
curl -X PUT --data-binary @serve.nu \
  https://http-nu-cf.gedw99.workers.dev/alice/admin/handler
```

## Build / CI status

- ✅ Desktop build / tests / examples: **unchanged.** `mise run ci` green.
- ✅ Curated Nu compiles to `wasm32-unknown-unknown` (gate test:
  `cargo build --target wasm32-unknown-unknown --lib --no-default-features`).
- ✅ Worker cdylib via `worker-build --features cloudflare`. Output
  `build/index_bg.wasm` is ~17MB raw / ~4.5MB brotli (fits Workers
  paid-tier).
- ✅ `wrangler dev` serves requests through real `crate::Engine`.
  Router DSL, HTML DSL, content-type inference, request body -> Nu
  `$in`, engine cache, streaming (`ListStream` / `ByteStream` via
  `worker::Response::from_stream`, `to sse`, `application/x-ndjson`),
  Datastar JS short-circuit (`include_bytes!`) all working.
- ✅ Per-user Workspace FS shipped (see "What works on the live worker"
  above). Filed [workers-rs#998](https://github.com/cloudflare/workers-rs/issues/998)
  asking Cloudflare to upstream it.

## Example status on CF (local wrangler dev)

Method: `mise run ex:cf:<name>` -> `curl http://127.0.0.1:8787/alice/...`.
Demos with non-Nu assets (templates, static files, JSON) need
`DEMO=<name> mise run cf:seed:demo` to upload those to the workspace
first. Last full sweep: see `scripts/cf-demos-probe.sh`.

| Example | Status | Notes |
|---|---|---|
| `blog` | ✅ works | Router DSL + HTML DSL. Self-contained, no seeding. |
| `basic` | ✅ works | All routes including `/time` (sleep is a no-op on CF; still streams). |
| `2048` | 🟡 partial | Home page renders correct HTML. Status code leaks 501 from `.static` (small bug). Gameplay over `.bus sub` blocked on cross-stream port. |
| `workspace-browser` | ✅ works | Designed for CF; R2 spill verified with 2MB file. |
| `datastar-counter` | ✅ works | Reactive counter, SSE round-trip. |
| `datastar-sdk` | ✅ works | SDK feature demo. |
| `datastar-sdk-test` | ✅ works | `/test` route requires a POST body (also true on desktop -- not a CF gap). |
| `generate-test` | ✅ works | Exercises stock `generate`. |
| `mermaid-editor` | ✅ works | Live editor; `source` was a non-issue in practice. |
| `tao` | ✅ works | Needs `DEMO=tao mise run cf:seed:demo` so `open data.json` / `.static /static/...` find content. Page renders styled with the demo's CSS. |
| `cargo-docs` | 🟡 code works | Returns 500 with `ls /target/doc: not found` until you seed cargo doc output into the workspace. The Nu code is fine; it's a data-prep gap. |
| `templates` | ❌ blocked | Top-level `.append page.html` (cross-stream). Needs xs CF backend before this parses. |
| `quotes` | ❌ blocked | `.last quotes --follow` / `.append quotes` (cross-stream). Same blocker as templates. |
| `stor` | ❌ blocked | `stor *` family unported to wasm. Port plan in [`src/cf/nu/nu_command/stor/README.md`](src/cf/nu/nu_command/stor/README.md). |
| `hub` (`examples/serve.nu`) | 🟡 bundler works | `scripts/bundle-cf-handler.nu` inlines `source X.nu` directives recursively (works -- bundled hub parses on desktop). Second blocker on CF: "External calls are not supported" because the bundled script references commands not registered on wasm. Untangling is the per-demo work above; once all demos are wasm-clean, the hub should follow. |

**Summary: 10 demos verified working on local wrangler dev; 1 (cargo-docs) needs data; 1 (2048) is partial; 3 (templates / quotes / stor) blocked on cross-stream / stor wasm ports.**

## What it would take to unblock the rest

Independent tracks, mostly outside the FS work:

1. **`.mj` (and other http-nu custom commands) routed through Vfs.**
   `.mj compile <path>` currently uses `std::fs::read_to_string` in
   `src/commands.rs`. Cfg-gate that call to use
   `crate::cf::vfs::with_vfs` on wasm. Unblocks `tao` and probably
   `templates`. Small, in-place patch.
2. **`stor` on wasm** -- port the `stor *` family + `query db` + the
   `sqlite-in-memory` custom value type. Backend choice is DO SQLite vs
   D1 (see `src/cf/nu/nu_command/stor/README.md`). Unblocks `stor`.
3. **xs CF backend** -- lives in the `xs` repo. Maps `fjall` -> DO
   SQLite, `cacache` -> R2. Unblocks `.bus`, `.cat`, `.append`, `.last`,
   `--store`, `--topic`, `--watch` reload. Unblocks `quotes`,
   `templates` (the `.append` path), and the streaming half of `2048`.
4. **`fetch` / `http get` / `http post` on wasm** -- blocked by the
   sync-Nu-eval / async-Workers-fetch mismatch. Same root as `sleep`.
   Fixes: (a) async Nu eval refactor upstream, OR (b) a side-channel
   `.fetch` custom command on the `RESPONSE_TX` pattern. Unblocks
   `2048`.
5. **`source` for hub / mermaid-editor** -- Nu's `source` resolves at
   parse time against the OS filesystem. Three real fixes: (a) patch
   Nu's parser to resolve `source` through a Vfs provider; (b)
   build-time preprocessor that inlines `source` statements before
   `include_str!`; (c) Workers-side bundler that pre-populates
   additional `include_str!` constants for every `source` target.

None of (1)-(5) are blocked by anything else; they're orthogonal work
tracks. None are tiny.
