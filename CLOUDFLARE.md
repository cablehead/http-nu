# http-nu on Cloudflare Workers

Port of http-nu to Cloudflare Workers via worker-rs. Branch lives at
`joeblew999/http-nu` (fork of cablehead/http-nu) and is structured so
upstream merges stay clean.

**Live:** https://http-nu-cf.gedw99.workers.dev (serves `examples/cf-workspace-browser`)

---

## Working practices (READ FIRST)

KISS rules. Follow these every session; details elsewhere in this doc
or in `CLAUDE.md`.

1. **Iterate with `mise run cf:dev`, not `cf:deploy`.** Local wrangler
   dev is ~3s/change; deploy is ~45s. Logs/panics print to terminal.
   First wasm build is slow; subsequent are fast.

2. **Grep `.src/` BEFORE writing new wasm/CF code.** Local clones of
   prior art:
   - `.src/nushell/` -- Nu upstream. `toolkit/wasm.nu` is the SSOT
     for which Nu crates compile to wasm32. Command sources at
     `crates/nu-command/src/<category>/<name>.rs` are what our
     `src/cf/commands/<category>/<name>.rs` shadows mirror file-for-file.
   - `.src/nu-on-web/` -- Nu-on-wasm32 (browser host). Cargo features
     recipe (`nu-command/js`), shadow command patterns, JS bridge.
   - `.src/agents/packages/shell/` -- `@cloudflare/shell` schema +
     semantics. Our `src/cf/workspace/` mirrors this byte-for-byte.
   - `.src/workers-rs/` -- Workers SDK. `sql.rs`, `durable.rs`.
   Adapt patterns; don't copy verbatim (host APIs differ).

3. **Shadow commands mirror Nu's source tree.** Path-for-path:
   ```
   nu-command/src/filesystem/ls.rs   ->  src/cf/commands/filesystem/ls.rs
   nu-command/src/path/exists.rs     ->  src/cf/commands/path/exists.rs
   nu-command/src/platform/sleep.rs  ->  src/cf/commands/platform/sleep.rs
   ```
   When Nu adds or restructures a stock command we want to shadow, add
   or move the equivalent file here in the same relative path. A diff
   between our tree and `.src/nushell/crates/nu-command/src/` shows
   command-for-command what changed.

4. **All CF-only code under `src/cf/`.** Never edit `src/lib.rs`,
   `src/handler.rs`, `src/commands.rs`, `src/response.rs` etc. for
   CF reasons. The Vfs trait lives in `src/cf/vfs.rs` for the same
   reason -- promote to top-level only when desktop actually opts in.

5. **Before shadowing a Nu command, check if a `nu-command` feature
   would register the stock one.** `nu-command/js` (already enabled)
   gives us `date now` / `format date` / etc. Stock beats home-rolled.

6. **`.static` uses the existing `RESPONSE_TX` pattern from
   `src/commands.rs`** -- don't reimplement. CF handler reads the
   channel after eval, serves bytes from Workspace + Content-Type
   from extension.

7. **Workspace schema is `@cloudflare/shell`-compatible.** Don't
   diverge from `cf_workspace_<ns>` columns/types or R2 key shape
   (`${prefix}/${ns}<path>`). Interop both ways with the JS package
   is the contract.

8. **`mise run cf:deploy` fetches the token from `fnox`.** No need
   to export `CLOUDFLARE_API_TOKEN` manually.

---

## What works on the live worker right now

- **Per-user routing** via the URL's first path segment: `/alice/...`
  lands in alice's DurableObject, `/bob/...` lands in bob's.
- **Per-user FS** backed by DO SQLite, via our Rust port of
  `@cloudflare/shell` at `src/cf/workspace/`. R2 spill at 1.5MB.
- **Nine Nu shadow commands** (`ls`, `open`, `save`, `mkdir`, `rm`,
  `cp`, `mv`, `path exists`, `glob`) read/write the per-request
  snapshot via the `Vfs` trait. Pending writes async-flush after eval.
- **`.static`** via the existing `RESPONSE_TX` pattern; serves from
  Workspace with Content-Type from extension.
- **`sleep`** as a CF-side no-op shadow (real sleep needs an async
  Nu eval path that doesn't exist yet).
- **Per-user handler hot-swap** via `PUT /<user>/admin/handler`.
- **Debug routes** `/<user>/_workspace/{ls,stat,cat,put,rm,mkdir}`.

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

## TL;DR

- One crate, two outputs. `cargo build` produces today's desktop
  binary unchanged. `mise run cf:build` produces a Workers cdylib.
- Cloudflare-only code is **additive** under `src/cf/`. Existing
  upstream files are byte-identical wherever possible; differences
  are inline `#[cfg(feature = "desktop")]` gates.
- The Workers entrypoint reuses `crate::Engine` directly -- no
  clean-room copy. Whatever custom commands http-nu has on desktop
  (`.bus pub`, `.mj`, `.md`, `.highlight`, `to sse`, ...) come along
  to CF automatically.

## Status

- ✅ Desktop build / tests / examples: **unchanged.** `mise run ci` green.
- ✅ Curated Nu compiles to `wasm32-unknown-unknown` (gate test:
  `cargo build --target wasm32-unknown-unknown --lib --no-default-features`).
- ✅ Worker cdylib builds via worker-build with `--features cloudflare`.
  `mise run cf:build` produces `build/index_bg.wasm` (~17MB raw,
  ~4.5MB brotli — fits Workers paid-tier comfortably).
- ✅ `wrangler dev` serves requests through real `crate::Engine`.
  `examples/blog/serve.nu` runs end-to-end on Workers:
  - `GET /` → HTML post list (200)
  - `GET /posts/getting-started-nushell` → single post page (200)
  - `GET /about` → about page (200)
  - Router DSL, HTML DSL, content-type inference all working.
- ✅ Request body → Nu `$in` pipeline (POST/PUT/PATCH bodies reach
  closures as a `ByteStream`).
- ✅ Engine cache (`OnceLock<Mutex<Engine>>`): warm requests reuse
  the parsed handler. Same property desktop has had since day one.
  No CF-only HTTP surface for hot reload -- waiting on a desktop-
  parity trigger (xs frame append, Workspace event); see
  "Handler script lifecycle" below.
- ✅ Streaming responses: `ListStream` and `ByteStream` flow through
  `worker::Response::from_stream`. `to sse` produces real chunked
  SSE output; record streams emit `application/x-ndjson`. Verified
  via wrangler dev.
- ✅ Datastar JS bundle short-circuit: `GET /datastar@1.0.1.js`
  returns the embedded bundle (`include_bytes!`) byte-identical to
  desktop's route at `src/handler.rs:148`.
- ✅ **Per-user Workspace FS, live**:
  - Rust port of `@cloudflare/shell` Workspace at [src/cf/workspace/](src/cf/workspace/).
    Schema-compatible (`cf_workspace_<ns>`), all FileSystem methods
    async, R2 spill at 1.5MB threshold (verified live with a 2MB file
    round-trip). R2 binding declared in [src/cf/wrangler.toml](src/cf/wrangler.toml).
  - One DurableObject per user via `USER_SPACE` binding; first URL
    path segment is the user_id.
  - Per-request `SnapshotVfs` preloaded from Workspace through a `Vfs`
    trait ([src/cf/vfs.rs](src/cf/vfs.rs)). Nine Nu shadow commands
    (`ls`, `open`, `save`, `mkdir`, `rm`, `cp`, `mv`, `path exists`,
    `glob`) call the trait via `with_vfs` ([src/cf/commands.rs](src/cf/commands.rs)).
    Writes buffer into pending ops and async-flush back to Workspace
    after Nu eval.
  - `.static` works via the existing `RESPONSE_TX` channel from
    [src/commands.rs](src/commands.rs) (no duplicate command). CF
    handler reads Workspace + sets Content-Type from extension.
  - Per-user handler hot-swap: `PUT /<user>/admin/handler` re-parses
    the closure for that user's DO isolate.
  - Filed [workers-rs#998](https://github.com/cloudflare/workers-rs/issues/998)
    asking Cloudflare to upstream this. Until/unless they do, our port
    lives here.

### Example status on CF (verified)

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

### What it would take to unblock the rest

These are independent tracks, mostly outside the FS work:

1. **nu-command/os parity on wasm** -- `sleep`, `generate`, `date now`,
   `format date`, `path self`. Upstream Nu work, or shadow each in
   `src/cf/commands.rs` (sleep needs a wasm-bindgen-futures async path
   reconciled with Nu's sync command surface; not trivial). Unblocks
   `basic`, `mermaid-editor` (partially), `2048` (partially).
2. **`stor` on wasm** -- enable `nu-command/sqlite` on wasm if it
   compiles cleanly, or shadow `stor` over the DO's `ctx.storage.sql`
   we already use for Workspace. Unblocks `stor`, and is a primitive
   `templates` / `quotes` could be ported onto if xs CF is delayed.
3. **BusDO with WebSocket Hibernation** -- new DO class in
   `src/cf/`, ~1-2 days. Unblocks `.bus sub`, the streaming half of
   `2048`.
4. **xs CF backend** -- lives in the `xs` repo. Maps `fjall` (LSM log)
   to DO SQLite and `cacache` (CAS) to R2. Days of work in that repo.
   Unblocks `--store`, `--topic`, `.cat`, `.append`, `.cas`, the
   `--watch` reload trigger on CF, and the rest of the `tao`/
   `quotes`/`templates` examples.
5. **`source` for hub** -- Nu's `source` resolves at parse time
   against the OS filesystem. Three real fixes (all real work):
   (a) patch Nu's parser to resolve `source` through a Vfs provider;
   (b) build-time preprocessor that inlines `source` statements into
   the handler script before `include_str!`; (c) Workers-side bundler
   that pre-populates additional `include_str!` constants for every
   `source` target.

None of (1)-(5) are blocked by anything else; they're orthogonal
work-tracks. None are tiny.

## Try it

```bash
mise install                          # one-time, all toolchain pins
mise run ci                           # verify desktop is green
mise run cf:build                     # build the Workers cdylib
mise run cf:dev                       # wrangler dev on :8787
curl http://127.0.0.1:8787/           # blog post list rendered by Nu

# Live tail logs from a deployed Worker:
mise run cf:tail
```

## What's here

```
src/                              cablehead/http-nu's tree (byte-identical
                                  layout; we add #[cfg(feature = "desktop")]
                                  gates inline where targets differ)
src/cf/                           CF-only code we own (never upstream)
  mod.rs                          #[event(fetch)] entrypoint + engine cache
  request.rs                      mirrors src/request.rs (worker::Request adapter)
  response.rs                     mirrors src/response.rs (PipelineData -> Response,
                                  streaming via worker::Response::from_stream)
  wrangler.toml                   Workers config
build/                            worker-build output (gitignored)
mise.toml                         tasks, including cf:build/cf:dev/cf:deploy
Cargo.toml                        single manifest with `desktop` (default),
                                  `cloudflare`, `cross-stream` features
```

### File-layout rule

Each file under `src/cf/` mirrors a sibling under `src/` when there's
a desktop equivalent. `src/cf/<x>.rs` is the wasm/CF flavor of
`src/<x>.rs`. Pair-comparison reviews are then a side-by-side diff per
file rather than a hunt across the tree.

| Situation | Where it goes |
|---|---|
| Helper used by *both* targets | upstream file (`src/<x>.rs`); both targets call it. Example: `src/response.rs::infer_content_type` is shared by `src/worker.rs` (desktop) and `src/cf/response.rs` (wasm). |
| CF adapter for a desktop concern | `src/cf/<same_name>.rs` (mirrors upstream filename) |
| Genuinely CF-only primitive (BusDO bridge, Vfs over `@cloudflare/shell`) | `src/cf/<descriptive>.rs` with a comment explaining why no upstream sibling |
| Desktop concern with no CF analog (e.g. `listener.rs` -- Workers invokes us, no listener) | upstream file gated `#[cfg(feature = "desktop")]`; *no* `src/cf/<same_name>.rs` |

When a CF helper *and* a desktop helper end up doing the same job, the
dedup goes upstream into `src/<x>.rs` and both targets call it. That
keeps duplication from accumulating.

The Workers entry (`src/cf/mod.rs`) calls `Engine::new()` +
`add_custom_commands()` + `parse_closure(...)` + `run_closure(...)`
-- the same surface desktop's `worker.rs` uses, just without the
thread spawn (eval runs sync inside the fetch handler for now;
async eval is an open design question).

The handler closure today is `include_str!("../../examples/blog/serve.nu")`
to exercise the real http-nu surface (router DSL, HTML DSL,
content-type inference) on a real example. Eventually it'll come
from R2 or `@cloudflare/shell`'s Workspace.

## Cross-repo boundary (http-nu vs xs)

http-nu and xs are two separate forks (cablehead/http-nu and
cablehead/xs); we maintain `joeblew999` branches on both. xs is the
persistent event-stream + CAS library that http-nu *depends on* for
`--store` / `--topic` / `.cat` / `.append` / `.cas`. The CF story
splits cleanly along the same dependency line:

- **This repo (http-nu) -- HTTP server concerns on CF:**
  the `#[event(fetch)]` entrypoint, request/response adapters,
  Datastar JS short-circuit, streaming bridges, BusDO for `.bus sub`,
  Vfs trait for `.static`. Anything that's about *serving HTTP from
  Nu closures* on Workers lives here, mostly under `src/cf/`.

- **xs repo -- storage/persistence CF backend:**
  swapping `fjall` (LSM index) for DO SQLite, swapping `cacache`
  (CAS) for R2. The xs::store / xs::api / xs::processor surfaces
  stay the same; what changes is the backend. `--topic`-loaded
  handlers, `.cat` / `.append` / `.cas`, and `--store`-using
  examples (quotes, templates) only work on CF once xs has that
  backend.

- **What lives at the seam:** in this repo, `src/store.rs` (and a
  future `src/cf/store.rs`) wires CF Workers' bindings through to
  xs. The actual storage code is xs's; we're the consumer.

- **Today:** no CF work has touched xs. The work in this repo so far
  (datastar JS, streaming, request/response adapters, engine cache,
  handler split) is purely HTTP-server-side. xs's repo does not
  need a single edit for that surface to be done.

**This file is the canonical CF design doc** for the joint http-nu +
xs CF effort. xs's repo has a one-line pointer back here -- when CF
work lands in xs, the design rationale lives here, the implementation
lives there.

## Coexistence rules (this is the merge story)

Upstream (`cablehead/http-nu`) keeps shipping. The two-axis split:

1. **Files that already exist upstream** (anything in `src/` other
   than `src/cf/`):
   - Never moved, renamed, or restructured.
   - Differences for CF land as `#[cfg(feature = "desktop")]` /
     `#[cfg(not(feature = "desktop"))]` gates **in place**.
   - Heavy desktop-only deps in `Cargo.toml` (hyper, rustls, ctrlc,
     notify, tower-http/fs, nu-cli, nu-plugin-engine, ...) are
     `optional = true` and pulled in only when the `desktop` feature
     is on. nu-* crate features (`os`, `network`, `rustls-tls`,
     `sqlite`, `plugin`) cascade through the `desktop` feature so
     `cargo build` builds desktop identically to before.

2. **Files that don't exist upstream** (new files we own):
   - Live under `src/cf/` (or sibling tooling).
   - Gated `#[cfg(all(feature = "cloudflare", target_arch = "wasm32"))]`
     so a desktop `cargo build --all-features` ignores them.
   - Free to evolve however we want -- upstream cannot conflict
     with a file it doesn't have.

### Merging upstream

```bash
git fetch upstream && git merge upstream/main
```

- Conflicts on a `src/*.rs` file we've cfg-gated: take upstream's
  logic, re-apply the gate. The gates are typically import lines or
  fn attributes -- small re-edits.
- New `pub mod foo;` from upstream in `src/lib.rs`: decide whether
  `foo` compiles to wasm32 cleanly (with `--no-default-features`).
  If desktop-only, gate the `pub mod` line.
- Run `mise run ci` (desktop) and `mise run cf:build` (wasm) to
  confirm both targets still pass. Push.

The merge cost is O(cfg-gate-edits), not O(architectural-decisions).

## Design notes (compressed)

### What was tried

- A workspace split (extract `http-nu-core`) was considered and
  rejected: relocating upstream files would conflict on every merge.
  Cfg-gating in place is uglier but cheaper to maintain.
- Cloudflare Containers / Sandbox SDK were considered and rejected.
  This is consistent with `../xs/CLOUDFLARE.md`'s position.
- A standalone `cf-spike/` crate was used briefly as a wasm
  compile-gate test; deleted once `src/cf/` could prove the same
  thing.

### Handler script lifecycle (embed now, swap at runtime later)

The Nu closure that handles each request is currently embedded at
build time:

- `src/cf/mod.rs` does `include_str!(env!("CF_HANDLER_PATH"))`.
- mise's `cf:build` and `ex:cf:<name>` tasks set `CF_HANDLER_PATH`
  to pick which `examples/<...>` script gets baked in.
- A new script ships only on the next deploy.

This is intentionally simple for the proof point. The architecture
supports runtime swap natively -- desktop already does it via
`ArcSwap<Engine>` so `--watch` and `--topic` can hot-reload the
handler without dropping connections. The CF side will land the same
shape; today's `include_str!` is the placeholder.

Sources we can plug a runtime-loaded handler into, cheapest first:

1. **POST `/admin/handler`** -- worker accepts the script as request
   body, stashes in DO storage or KV, ArcSwap-replaces the engine.
   Live edit via `curl`. ~50 lines.
2. **KV** -- boot reads `KV.get("handler")` once, refreshes on a
   schedule. `wrangler kv put` to update. Cheap, eventually
   consistent.
3. **R2** -- same shape as KV, fits bigger scripts.
4. **`@cloudflare/shell` Workspace + git** -- worker `git pull`s a
   handler repo on an alarm tick. Versioned. The closest CF analog
   to desktop's `--watch` against a checkout. Larger commit.
5. **Per-request override** -- a header (e.g. `X-Handler-Id`)
   selects from a namespace. Multi-tenant story.

(1) is the smallest meaningful unlock; it gets you live editing in a
single short PR. (4) is the closest match to the local-first
"point http-nu at a directory" experience.

### What we'll need (not yet built)

- `Vfs` trait (path-keyed FS abstraction) with desktop and CF
  impls. Desktop = `tokio::fs` + `notify`. CF = `@cloudflare/shell`
  Workspace (DO SQLite + R2). Required when `.static` /
  `--watch` / `--topic` move across.
  Note: bare R2 is ruled out. R2 is object storage with flat keys;
  you can fake directory listing with prefix queries but you cannot
  give Nushell the POSIX-like `stat` / `readdir` semantics its fs
  commands actually call. Workspace provides a real FS index (DO
  SQLite) with R2 for blob storage -- that is the correct substrate.

  Implementation plan -- preload + sync read, using `@cloudflare/shell`:

  **The constraints:**
  - Nu commands (`ls`, `open`, `path exists`) are synchronous; they
    cannot `await`.
  - `WorkspaceFileSystem` (`@cloudflare/shell`) is fully async. So is
    R2. `SqlStorage::exec()` in workers-rs is sync, but Workspace
    spills files >1.5MB to R2 (async) and resolves symlinks (up to
    40-deep). Going around Workspace via raw SQL means reimplementing
    ~1800 lines of edge cases (R2 spill, symlink walks, encoding,
    glob, namespace prefixing, schema migration). That is exactly
    the "reinventing the wheel" trap.

  **The pick:** use `@cloudflare/shell` via an async JS bridge, and
  decouple the async I/O from Nu eval with a per-request preload:

  1. **Worker prelude (JS, async)** -- before invoking the Wasm
     handler, walk the handler's known dependencies (handler script
     itself, any `.static`/`source`/`open` targets the request needs)
     and `await ws.readFile()` / `ws.readdir()` for each. Build a
     `{ path -> bytes }` snapshot plus a `{ path -> stat }` map.
  2. **Hand the snapshot to Rust** via `wasm_bindgen`. Snapshot is a
     `HashMap<String, Vec<u8>>` plus stat map, owned by Rust for the
     request lifetime.
  3. **`src/cf/vfs.rs` (Rust, sync)** -- reads against the snapshot.
     No JS calls during Nu eval. No `SqlStorage::exec()`. No async.
  4. **`src/cf/commands/`** -- Nu shadow commands (`ls`, `open`,
     `path exists`) call `vfs.rs` against the snapshot. Same shape
     as `.src/nu-on-web/src/commands/`.
  5. **Writes** (rare in handlers) -- collect into a pending-writes
     buffer in the snapshot; flush back out through async JS calls
     to `ws.writeFile()` after Nu eval returns. Or reject writes
     entirely in the initial version.

  The async cost lives in the prelude, not in Nu eval. All the hard
  storage logic (R2 spill, symlinks, encoding, glob) stays in
  `@cloudflare/shell` where it's already debugged.

  **Dep discovery for the prelude.** Two strategies, cheapest first:
  - Static: parse the handler closure once at warm-up, collect
    literal paths it references (`open "foo.html"`, `.static "dir"`).
    Cache as `HashSet<PathBuf>` keyed by handler hash. Preload that
    set per request. Misses dynamic paths.
  - Dynamic: run Nu, catch ENOENT, async-fetch the missing path,
    retry. Slow but correct. Use as a fallback after static.

  Static covers the realistic case (a blog handler `open`s the same
  template set every request). Dynamic is the escape hatch.

  Management plane stays in JS (`@cloudflare/shell`): git pull, diff,
  archive, search/replace, glob, structured edit planning all run
  via `Workspace` directly. They are async. Run in a management DO
  or between requests. Same DO SQLite + R2 backing; whatever git
  pull writes is visible to the next request's prelude.

  Once proven, `vfs.rs` + `commands/` + the prelude shim extract to
  a `cf-vfs` crate in a shared platform repo; other Rust Workers
  that want Nu-style fs over Workspace depend on it via Cargo git
  dep.

  Deeper insight: the desktop Vfs (tokio::fs + notify) and the CF
  Vfs (@cloudflare/shell Workspace) are not just compatible -- they
  are the same concept with different backends:

  | Primitive     | Desktop                  | CF Workers                  |
  |---------------|--------------------------|-----------------------------|
  | File storage  | local fs (tokio::fs)     | Workspace (DO SQLite + R2)  |
  | Git           | local git                | isomorphic-git (@cf/shell)  |
  | Change signal | notify (fs watch)        | DO alarm / --topic event    |

  This means sync is achievable: `git push` from desktop → CF
  Workspace picks it up → handler hot-reloads. That is the CF
  equivalent of `--watch` on desktop, with git as the transport
  instead of inotify. The Vfs trait abstraction is what makes both
  sides substitutable -- the Nu script never knows which backend it
  is talking to.
- `BusBridge` for `.bus sub` -- desktop uses thread + tokio runtime
  (gated, today's behavior); CF will use a Durable Object with
  WebSocket Hibernation. Both emit the same record stream.
- Async eval refactor. `Engine::run_closure` is sync. On Workers
  long evals hold the isolate; SSE handlers have to yield. Probably
  means rewriting `src/worker.rs`'s eval loop (gated as desktop
  today) to an async variant for CF.
- Storage primitives mapping: xs's `fjall` log + `cacache` CAS
  map ~1:1 to DO SQLite + R2 via `@cloudflare/shell`'s `Workspace`.
  The eventual `--store` story on CF reuses that.

## Acknowledgements

The wasm path is well-trodden by the upstream Nushell team:

- [`nushell/nushell` `toolkit/wasm.nu`](https://github.com/nushell/nushell/blob/main/toolkit/wasm.nu)
  is the SSOT for which Nu crates compile to wasm32. They CI-check
  it with `cargo clippy --target wasm32-unknown-unknown
  --no-default-features -- -D warnings -D clippy::unwrap_used` per
  crate. We follow that list.
- [`@cptpiepmatz`](https://github.com/cptpiepmatz) drives upstream's
  wasm work; worth tracking before assuming a wasm gap is permanent.
- [`nu-on-web/nu-on-web`](https://github.com/nu-on-web/nu-on-web)
  ships Nushell in a browser today. Their Cargo recipe was the
  template for ours (`nu-command` with `default-features = false`,
  `features = ["js", "rand"]` + `getrandom/wasm_js` +
  `console_error_panic_hook`). Critically, their `src/zenfs.rs`
  (local copy: `.src/nu-on-web/src/zenfs.rs`) is the `wasm_bindgen`
  extern pattern for shadowing Nu's fs commands via a JS VFS backend
  (`@zenfs/core`). Our `src/cf/vfs.rs` will follow the same pattern
  targeting `@cloudflare/shell`'s `WorkspaceFileSystem` instead.
  Their `src/commands/` (ls, cat, rm) is the shadow command template.
- [`@cloudflare/shell`](https://www.npmjs.com/package/@cloudflare/shell)
  -- Workers-native FS + `isomorphic-git` package. Provides
  `WorkspaceFileSystem` (DO SQLite + R2) and `InMemoryFs`. Local copy
  of the README: `.src/agents/packages/shell/README.md`. Will be the
  JS-side substrate for our `Vfs` impl; Rust calls into it via
  `wasm_bindgen` externs in `src/cf/vfs.rs`.

## Questions for review

- The `desktop` feature cascade adds ~30 lines to `Cargo.toml` and
  ~10 cfg gates inside `src/`. Acceptable as upstream contributions,
  or too noisy? Alternative would be to keep the fork and not
  upstream the CF support at all.
- `src/cf/` as a place for CF-only code -- consistent with the
  existing `src/stdlib/` convention, or wrong-shaped?
- `src/cf/wrangler.toml` is unconventional (worker-rs users expect
  it at repo root). It's there to keep CF artifacts together;
  mise tasks abstract the path.
- Curated Nu deps in `Cargo.toml` (`default-features = false` on
  `nu-protocol` etc.) -- is this safe to merge into upstream as a
  precondition for the wasm branch, or do upstream desktop builds
  rely on something we'd be stripping?
