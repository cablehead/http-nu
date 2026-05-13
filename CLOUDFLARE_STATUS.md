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
| `basic` | 🟡 partial | `/`, `/hello`, `/json`, `/echo`, `/info` all work. `/time` is BROKEN on CF: it uses `generate { sleep 1sec ... } true` which never terminates because our `sleep` shadow is a no-op. Spin-loops until the Worker hits its CPU budget and wrangler dies. Needs async Nu eval to fix properly; for now, avoid the route. |
| `2048` | 🟡 partial | Home page + `/og.png` both GET 200 with correct content (after `DEMO=2048 mise run cf:seed:demo`). Earlier 501 reports were `curl -I` (HEAD) artifacts -- routes declare `method: GET`, so HEAD legitimately 501s. Gameplay over `.bus sub` is the real CF blocker. |
| `workspace-browser` | ✅ works | Designed for CF; R2 spill verified with 2MB file. |
| `datastar-counter` | ✅ works | Reactive counter, SSE round-trip. |
| `datastar-sdk` | ✅ works | SDK feature demo. |
| `datastar-sdk-test` | ✅ works | `/test` route requires a POST body (also true on desktop -- not a CF gap). |
| `generate-test` | ✅ works | Exercises stock `generate`. |
| `mermaid-editor` | ✅ works | Live editor; `source` was a non-issue in practice. |
| `tao` | ✅ works | Needs `DEMO=tao mise run cf:seed:demo` so `open data.json` / `.static /static/...` find content. Page renders styled with the demo's CSS. |
| `cargo-docs` | ✅ works | `mise run cf:seed:cargo-docs` runs `cargo doc --workspace --no-deps` then uploads target/doc to /alice/target/doc. Index page + per-crate rustdoc pages render. Needed an index.html fallback in `.static` for directory-style requests (now in handler.rs). |
| `templates` | ❌ blocked | Top-level `.append page.html` (cross-stream). Needs xs CF backend before this parses. |
| `quotes` | ❌ blocked | `.last quotes --follow` / `.append quotes` (cross-stream). Same blocker as templates. |
| `stor` | ❌ blocked | `stor *` family unported to wasm. Port plan in [`src/cf/nu/nu_command/stor/README.md`](src/cf/nu/nu_command/stor/README.md). |
| `hub` (`examples/serve-cf.nu`) | ✅ works | CF-tailored hub at `examples/serve-cf.nu` -- same shape as desktop `examples/serve.nu` but only `source`s the 11 wasm-clean demos. `mise run cf:dev:hub` builds the bundled handler via `scripts/bundle-cf-handler.nu`. All 10 mounted sub-demos return 200 on `GET /alice/<demo>/...`. Per-demo asset routes (tao's CSS, cargo-docs's per-crate pages) still expect root-relative URLs and won't traverse the mount prefix; that's a separate per-demo `$req.mount_prefix` adoption. |

**Summary: 11 demos verified working on local wrangler dev. 2048 home + assets work; gameplay (`.bus sub`) needs streaming bridge. 3 demos (templates / quotes / stor) blocked on cross-stream / stor wasm ports.**

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
