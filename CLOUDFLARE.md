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
- ⏳ Not yet wired: HTTP status codes from `metadata set` (nonexistent
  routes return 200 instead of 404; need response-metadata bridging
  in `src/cf/mod.rs`); request body → Nu pipeline; `ListStream` /
  `ByteStream` → JS `ReadableStream`; `.static` (Vfs); `.bus sub`
  on a Bus DO; loading handler from R2 / `@cloudflare/shell`.

## Try it

```bash
mise install                          # one-time, all toolchain pins
mise run ci                           # verify desktop is green
mise run cf:build                     # build the Workers cdylib
mise run cf:dev                       # wrangler dev on :8787
curl http://127.0.0.1:8787/           # blog post list rendered by Nu
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
