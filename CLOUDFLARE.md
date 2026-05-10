# http-nu on Cloudflare Workers

Work-in-progress port of http-nu to Cloudflare Workers via worker-rs.
This branch lives at `joeblew999/http-nu` (fork of cablehead/http-nu)
and is structured so upstream merges stay clean.

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
- ✅ Runtime handler swap via `PUT /admin/handler` (option 1 from the
  lifecycle ladder below). Engine is cached in `OnceLock<Mutex<...>>`;
  bad scripts return 400 and leave the running handler intact. New
  script sticks for the lifetime of the warm isolate. ⚠ unauth'd --
  gate before deploy.
- ⏳ Not yet wired: streaming responses (`ListStream` / `ByteStream`
  → JS `ReadableStream`); `.static` (Vfs); `.bus sub` on a Bus DO;
  persistent handler (today's swap survives only warm requests; cold
  starts revert to embedded -- options 2-4 in the lifecycle ladder
  add KV / R2 / Workspace persistence).
- ⏳ The `examples/serve.nu` hub (`mise run cf:build` with
  `CF_HANDLER_PATH=../../examples/serve.nu`) **fails to parse** on
  Workers: it does `source basic.nu` etc., which resolves through
  the filesystem. On wasm there's no fs so each `source` returns
  `SourcedFileNotFound`. This is the canonical use case for the Vfs
  trait + `@cloudflare/shell`'s Workspace -- back the hub's
  `source` calls with R2/Workspace and the hub works.

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
  mod.rs                          #[event(fetch)] entrypoint -> Engine
  wrangler.toml                   Workers config
build/                            worker-build output (gitignored)
mise.toml                         tasks, including cf:build/cf:dev/cf:deploy
Cargo.toml                        single manifest with `desktop` (default),
                                  `cloudflare`, `cross-stream` features
```

The Workers entry (`src/cf/mod.rs`) calls `Engine::new()` +
`add_custom_commands()` + `parse_closure(...)` + `run_closure(...)`
-- the same surface desktop's `worker.rs` uses, just without the
thread spawn (eval runs sync inside the fetch handler for now;
async eval is an open design question).

The handler closure today is `include_str!("../../examples/blog/serve.nu")`
to exercise the real http-nu surface (router DSL, HTML DSL,
content-type inference) on a real example. Eventually it'll come
from R2 or `@cloudflare/shell`'s Workspace.

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
  `console_error_panic_hook`).
- [`@cloudflare/shell`](https://www.npmjs.com/package/@cloudflare/shell)
  -- Workers-native FS + `isomorphic-git` package. Lives inside the
  isolate (no Containers). Will be the substrate for our `Vfs`
  impl when that lands.

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
