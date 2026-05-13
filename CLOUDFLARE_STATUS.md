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
- `@cloudflare/shell` Rust port: [`src/cf/shell/PORT_STATUS.md`](src/cf/shell/PORT_STATUS.md)

## What works on the live worker

- **Per-user routing** via the URL's first path segment: `/alice/...`
  lands in alice's DurableObject, `/bob/...` lands in bob's.
- **Per-user FS** backed by DO SQLite + R2 spill, via the
  `@cloudflare/shell` Rust port at
  [`src/cf/shell/`](src/cf/shell/README.md). R2 spill at 1.5MB
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
  The `conformance` route runs `crate::shell::conformance`'s generic
  `FileSystem` suite against the real `Workspace`. `200` + `<n> passed`
  body means every assertion holds; `500` + backtrace means the first
  assertion that failed. This is the wasm-side leg of the mock-divergence
  defence -- the InMemoryFs desktop tests + this route together prove
  the two backends agree.

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

## Example status on CF (verified)

`mise run cf:deploy` with `CF_HANDLER_PATH=examples/<name>/serve.nu`,
then curl-tested. **Verified** means deployed + GET / returns 200 with
expected body shape; not a deep functional test.

| Example | Status | Notes |
|---|---|---|
| `blog` | ✅ verified | Router DSL + HTML DSL. |
| `cf-workspace-browser` | ✅ verified | Uses the shadow command set. R2 spill verified with 2MB file. |
| `datastar-counter` | ✅ verified | Datastar JS + Nu state. |
| `datastar-sdk` | ✅ verified | Datastar SDK demo. |
| `basic` | ✅ verified | `/`, `/hello`, `/json`, `/info` all 200 with correct content. `/time` would spin (uses `generate { sleep 1sec ...}` and sleep is a CF no-op). Unblocked by the path-strip patch + the demonstrated fact that `generate` / `date now` / `format date` work via stock + `nu-command/js`. |
| `cargo-docs` | ✅ parse-verified, needs files | Parses + serves a 500 when `target/doc/*` is empty -- expected behaviour. Upload doc files via `/<user>/_workspace/put` (no bundled tool yet) and the index page renders. Strategy works; just needs content. |
| `mermaid-editor` | ❌ blocked at parse | Uses `source` -> Nu resolves at parse time against the host filesystem (Vfs hookup needed in upstream Nu parser). |
| `2048` | ❌ blocked at parse | `fetch` (async-only on Workers, sync Nu can't call it). `sleep` is a CF no-op, would spin. |
| `tao` | ⚠️ partially unblocked | Path-strip + `$HTTP_NU` const set fixed parse for `use http-nu/router *` / `http *`. Files uploaded to workspace (`data.json`, `page.html`) via `_workspace/put`. NEW remaining blocker: `.mj compile` (http-nu's MJML custom command) reads template via `std::fs::read_to_string` instead of Vfs. CF-fixable: cfg-gate the file read in `src/commands.rs` to route through `crate::cf::vfs::with_vfs` on wasm. |
| `stor` | ❌ blocked at parse | `stor *` family absent on wasm (`nu-command/sqlite` off because `rusqlite` won't compile). Port plan + backend tradeoff in [`src/cf/nu/nu_command/stor/README.md`](src/cf/nu/nu_command/stor/README.md). |
| `templates` | ❌ blocked at parse | Same `.append` xs blocker as quotes; even when gated by `if $HTTP_NU.store != null`, Nu parses the body. |
| `quotes` | ❌ blocked at parse | `.last` xs streaming command. Needs xs CF backend (xs repo). |
| `hub` (`examples/serve.nu`) | ❌ blocked at parse | Uses Nu `source basic.nu` -> `SourcedFileNotFound`. Nu resolves `source` at parse time against the host filesystem. |

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
