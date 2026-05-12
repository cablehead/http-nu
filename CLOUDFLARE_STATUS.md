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
| `mermaid-editor` | ❌ blocked at parse | Uses `path self` -> `$env.PWD is not an absolute path` (wasm runtime has no PWD). |
| `cargo-docs` | ⚠️ untested | Should serve via `.static` over Workspace once doc files are uploaded into `/<user>/_workspace/put`. No bundled upload tool yet. |
| `basic` | ❌ blocked at parse | `date now`, `format date`, `sleep`, `generate` are missing from the wasm Nu surface (`nu-command/os` is off). Nu treats them as external calls -> "External calls are not supported." |
| `2048` | ❌ blocked at parse | `.bus sub` + `sleep` + `generate`. Needs BusDO with WS Hibernation + sleep/generate parity. |
| `tao` | ❌ blocked | `--dev -w` watch mode. Needs DO alarm + Workspace change events. |
| `stor` | ❌ blocked at parse | `stor` command from `nu-command/sqlite`, off on wasm. Could be added with `nu-command/sqlite` enabled on wasm (if it compiles) or a CF-side `stor` shadow over DO SQLite. |
| `templates` | ❌ blocked | Uses `--store`, needs xs CF backend (xs repo, not http-nu). |
| `quotes` | ❌ blocked | Same `--store` dependency as templates. |
| `hub` (`examples/serve.nu`) | ❌ blocked at parse | Uses Nu `source basic.nu` -> `SourcedFileNotFound`. Nu resolves `source` at parse time against the host filesystem, which doesn't exist on wasm. |

## What it would take to unblock the rest

Independent tracks, mostly outside the FS work:

1. **nu-command parity on wasm** -- `sleep`, `generate`, `date now`,
   `format date`, etc. Either upstream Nu work or new shadows in
   `src/cf/nu/nu_command/`. The shadow-side gaps (with reasons each one
   would matter) are tracked in
   [`src/cf/nu/nu_command/PORT_STATUS.md`](src/cf/nu/nu_command/PORT_STATUS.md)'s
   "Gaps / next port targets" section. Unblocks `basic`,
   `mermaid-editor` (partially), `2048` (partially).
2. **`stor` on wasm** -- enable `nu-command/sqlite` on wasm if it
   compiles, or shadow `stor` over the DO's `ctx.storage.sql` we
   already use for Workspace. Unblocks `stor`, and is a primitive
   `templates` / `quotes` could be ported onto if xs CF is delayed.
3. **BusDO with WebSocket Hibernation** -- new DO class in `src/cf/`,
   ~1-2 days. Unblocks `.bus sub`, the streaming half of `2048`.
4. **xs CF backend** -- lives in the `xs` repo. Maps `fjall` (LSM log)
   to DO SQLite and `cacache` (CAS) to R2. Days of work in that repo.
   Unblocks `--store`, `--topic`, `.cat`, `.append`, `.cas`, the
   `--watch` reload trigger on CF, and the `tao`/`quotes`/`templates`
   examples.
5. **`source` for hub** -- Nu's `source` resolves at parse time against
   the OS filesystem. Three real fixes: (a) patch Nu's parser to
   resolve `source` through a Vfs provider; (b) build-time preprocessor
   that inlines `source` statements before `include_str!`; (c)
   Workers-side bundler that pre-populates additional `include_str!`
   constants for every `source` target.

None of (1)-(5) are blocked by anything else; they're orthogonal work
tracks. None are tiny.
