# http-nu on Cloudflare Workers

Port of http-nu to Cloudflare Workers via worker-rs. Branch lives at
`joeblew999/http-nu` (fork of cablehead/http-nu) and is structured so
upstream merges stay clean.

This file is the **durable design doc**: merge story, cross-repo
boundary with xs, Vfs/handler-lifecycle design, acknowledgements,
open questions. Anything that drifts (what works on the live worker,
example status, what's blocking what) lives in
[`CLOUDFLARE_STATUS.md`](CLOUDFLARE_STATUS.md).

---

## Where to look first

For the two largest in-tree subsystems the **per-folder docs are
canonical**; don't restate contents here, point at them:

- **Nu shadow commands** (`ls`, `open`, `save`, `path self`, `sleep`,
  ...) -- see [`src/cf/nu/nu_command/README.md`](src/cf/nu/nu_command/README.md)
  for the durable overview,
  [`src/cf/nu/nu_command/CLAUDE.md`](src/cf/nu/nu_command/CLAUDE.md) for the
  contributor checklist (file-layout rule, registration step,
  Vfs-only rule),
  [`src/cf/nu/nu_command/PORT_STATUS.md`](src/cf/nu/nu_command/PORT_STATUS.md)
  for the running shadow table.
- **`@cloudflare/shell` Rust port.** Two workspace crates:
  - **`cloudflare-shell`** (backend-agnostic: FileSystem trait,
    FsError, path_utils, generic conformance suite) -- reusable from
    any Rust project. See
    [`crates/cloudflare-shell/README.md`](crates/cloudflare-shell/README.md)
    / [`CLAUDE.md`](crates/cloudflare-shell/CLAUDE.md).
  - **`cloudflare-shell-workspace`** (wasm-only Workspace impl: DO
    SQLite + R2 spill + schema). See
    [`crates/cloudflare-shell-workspace/README.md`](crates/cloudflare-shell-workspace/README.md)
    / [`CLAUDE.md`](crates/cloudflare-shell-workspace/CLAUDE.md) /
    [`PORT_STATUS.md`](crates/cloudflare-shell-workspace/PORT_STATUS.md).
    `PORT_STATUS.md` is the upstream coverage ledger spanning BOTH
    crates.

For running state at the project level (live worker, example matrix,
build/CI green checks, unblock tracks):
[`CLOUDFLARE_STATUS.md`](CLOUDFLARE_STATUS.md).

## Working practices

`CLAUDE.md`'s "CF Worker development workflow" section is the canonical
checklist (iterate with `cf:dev` not `cf:deploy`, grep `.src/` before
greenfield, never edit `src/*.rs` for CF reasons, per-demo desktop/CF
parity check). One design rule specific to this doc:

- **`.static` reuses `RESPONSE_TX`** from `src/commands.rs`; don't
  reimplement. The CF handler reads the channel after eval, serves
  bytes from Workspace with Content-Type from extension.

Shadow-command and shell-port rules live in their respective folder
`CLAUDE.md`s -- don't duplicate them here.

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

For what's actually green on the live worker today, the example
matrix, and the orthogonal tracks needed to unblock the rest, see
[`CLOUDFLARE_STATUS.md`](CLOUDFLARE_STATUS.md).

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

## Testing (desktop/CF parity)

Each example must behave the same on desktop and CF -- they are the
same Nu source. Per-demo parity check is mandatory before claiming a
demo "works on CF":

```bash
# a) Desktop baseline
mise run ex:<name>                                                 # serves at :3001
curl -i http://127.0.0.1:3001/                                     # capture HTTP code, body, Content-Type

# b) CF local (must match (a) before remote)
CF_HANDLER_PATH=examples/<name>/serve.nu mise run cf:dev
curl -i http://127.0.0.1:8787/                                     # diff against (a)

# c) CF remote (only after (b) matches)
CF_HANDLER_PATH=examples/<name>/serve.nu mise run cf:deploy
curl -i https://http-nu-cf.gedw99.workers.dev/
```

If (b) diverges from (a), fix the *cause* (commonly: a
wasm-incompatible Nu command, `$env.PWD` path resolution, a missing
workspace file). Don't paper over by changing the example -- the demo
is the spec.

Documented exceptions (also flagged in
[`CLOUDFLARE_STATUS.md`](CLOUDFLARE_STATUS.md)'s example table):
`sleep` is a no-op on CF until async Nu eval lands; `path self`
returns a workspace-rooted path (same semantic as desktop, different
string). Anything else: parity.

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
  nu/                             Nushell-side ports / shadows for the CF target.
                                  Mirrors `.src/nushell/crates/` -- one folder
                                  per nu-* crate (snake-case for Rust modules).
    nu_command/                   Mirrors `nu-command`. Nu shadow commands
                                  (filesystem, path, platform). See its
                                  README/CLAUDE/PORT_STATUS for the layout
                                  rule, contributor checklist, and current
                                  shadow table.
  shell/                          Rust port of @cloudflare/shell. See its
                                  README/CLAUDE/PORT_STATUS for upstream mapping,
                                  schema interop contract, and porting ledger.
  vfs.rs                          Vfs trait + SnapshotVfs (sync view for Nu eval)
  wrangler.toml                   Workers config
build/                            worker-build output (gitignored)
mise.toml                         tasks, including cf:build/cf:dev/cf:deploy
Cargo.toml                        single manifest with `desktop` (default),
                                  `cloudflare`, `cross-stream` features
```

### File-layout rule

Each file under `src/cf/` mirrors a sibling under `src/` when there's a
desktop equivalent. `src/cf/<x>.rs` is the wasm/CF flavor of
`src/<x>.rs`. Pair-comparison reviews are a side-by-side diff per file
rather than a hunt across the tree.

| Situation | Where it goes |
|---|---|
| Helper used by *both* targets | upstream file (`src/<x>.rs`); both targets call it. Example: `src/response.rs::infer_content_type` is shared by `src/worker.rs` (desktop) and `src/cf/response.rs` (wasm). |
| CF adapter for a desktop concern | `src/cf/<same_name>.rs` (mirrors upstream filename) |
| Genuinely CF-only primitive (BusDO bridge, Vfs over `@cloudflare/shell`) | `src/cf/<descriptive>.rs` with a comment explaining why no upstream sibling |
| Desktop concern with no CF analog (e.g. `listener.rs` -- Workers invokes us, no listener) | upstream file gated `#[cfg(feature = "desktop")]`; *no* `src/cf/<same_name>.rs` |

When a CF helper *and* a desktop helper end up doing the same job, the
dedup goes upstream into `src/<x>.rs` and both targets call it.

The Workers entry (`src/cf/mod.rs`) calls `Engine::new()` +
`add_custom_commands()` + `parse_closure(...)` + `run_closure(...)` --
the same surface desktop's `worker.rs` uses, just without the thread
spawn (eval runs sync inside the fetch handler for now; async eval is
an open design question).

## Cross-repo boundary (http-nu vs xs)

http-nu and xs are two separate forks (cablehead/http-nu and
cablehead/xs); we maintain `joeblew999` branches on both. xs is the
persistent event-stream + CAS library that http-nu *depends on* for
`--store` / `--topic` / `.cat` / `.append` / `.cas`. The CF story
splits cleanly along the same dependency line:

- **This repo (http-nu) -- HTTP server concerns on CF:** the
  `#[event(fetch)]` entrypoint, request/response adapters, Datastar JS
  short-circuit, streaming bridges, BusDO for `.bus sub`, Vfs trait for
  `.static`. Anything that's about *serving HTTP from Nu closures* on
  Workers lives here, mostly under `src/cf/`.
- **xs repo -- storage/persistence CF backend:** swapping `fjall` (LSM
  index) for DO SQLite, swapping `cacache` (CAS) for R2. The xs::store
  / xs::api / xs::processor surfaces stay the same; what changes is
  the backend. `--topic`-loaded handlers, `.cat` / `.append` / `.cas`,
  and `--store`-using examples (quotes, templates) only work on CF
  once xs has that backend.
- **What lives at the seam:** in this repo, `src/store.rs` (and a
  future `src/cf/store.rs`) wires CF Workers' bindings through to xs.
  The actual storage code is xs's; we're the consumer.
- **Today:** no CF work has touched xs. The work in this repo so far
  is purely HTTP-server-side. xs's repo does not need a single edit
  for that surface to be done.

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
   - Free to evolve however we want -- upstream cannot conflict with a
     file it doesn't have.

### Merging upstream

```bash
git fetch upstream && git merge upstream/main
```

- Conflicts on a `src/*.rs` file we've cfg-gated: take upstream's
  logic, re-apply the gate. The gates are typically import lines or
  fn attributes -- small re-edits.
- New `pub mod foo;` from upstream in `src/lib.rs`: decide whether
  `foo` compiles to wasm32 cleanly (with `--no-default-features`). If
  desktop-only, gate the `pub mod` line.
- Run `mise run ci` (desktop) and `mise run cf:build` (wasm) to
  confirm both targets still pass. Push.

The merge cost is O(cfg-gate-edits), not O(architectural-decisions).

## Design notes (compressed)

### What was tried

- A workspace split (extract `http-nu-core`) was considered and
  rejected: relocating upstream files would conflict on every merge.
  Cfg-gating in place is uglier but cheaper to maintain.
- Cloudflare Containers / Sandbox SDK were considered and rejected.
  Consistent with `../xs/CLOUDFLARE.md`'s position.
- A standalone `cf-spike/` crate was used briefly as a wasm
  compile-gate test; deleted once `src/cf/` could prove the same.

### Handler script lifecycle

Today: `src/cf/mod.rs` does `include_str!(env!("CF_HANDLER_PATH"))`;
mise's `cf:build` and `ex:cf:<name>` tasks set the path. A new script
ships on the next deploy.

Live edit, two paths today:

1. **`PUT /<user>/admin/handler`** -- worker accepts the script as
   request body, re-parses the closure directly into that user's DO
   engine cache. ~50 lines, exactly the desktop `ArcSwap<Engine>`
   pattern adapted per-user.
2. **Workspace write to `/serve.nu`** -- the user's `Workspace`
   `onChange` fires; an `AtomicBool` flag flips; the next request
   reads `/serve.nu` from Workspace and re-parses into the engine
   cache. This is the *event-driven* path; any write source (Nu
   shadow `save`, debug `/_workspace/put`, future `git pull` once
   the git/ port lands) goes through the same signal.

The two paths cooperate: PUT/admin/handler is the "tell me explicitly"
shape; the Workspace-write path is the "I'll notice on my own" shape.
Both end up calling `engine.parse_closure()` on the same per-DO
cached engine.

Future variants if we need them: KV (boot reads `KV.get("handler")`,
refresh on schedule), R2 (same shape, fits bigger scripts),
`@cloudflare/shell` Workspace + git pull on alarm tick (closest match
to desktop's `--watch` against a checkout; needs the `git/` port and
the xs CF backend).

### Vfs trait: desktop/CF symmetry

The desktop Vfs (tokio::fs + notify) and the CF Vfs (`@cloudflare/shell`
Workspace) are the same concept with different backends:

| Primitive     | Desktop                  | CF Workers                  |
|---------------|--------------------------|-----------------------------|
| File storage  | local fs (tokio::fs)     | Workspace (DO SQLite + R2)  |
| Git           | local git                | isomorphic-git (@cf/shell)  |
| Change signal | notify (fs watch)        | DO alarm / --topic event    |

Sync is achievable: `git push` from desktop -> CF Workspace picks it
up -> handler hot-reloads. The CF equivalent of `--watch` on desktop,
with git as the transport instead of inotify.

How the sync constraint shapes the CF impl: Nu commands (`ls`, `open`,
`path exists`) are synchronous; `WorkspaceFileSystem` and R2 are async.
Solution: per-request preload (async JS prelude reads the dependency
set from Workspace into a `HashMap<String, Vec<u8>>`), hand to Rust
via `wasm_bindgen`, Nu eval reads against the snapshot synchronously,
buffered writes async-flush back to Workspace after eval returns. The
hard storage logic (R2 spill, symlinks, encoding, glob) stays in
`@cloudflare/shell` where it's already debugged.

Bare R2 was ruled out for the FS substrate -- R2 is object storage
with flat keys; you can fake directory listing with prefix queries but
you cannot give Nushell the POSIX-like `stat` / `readdir` semantics
its fs commands actually call. Workspace provides a real FS index (DO
SQLite) with R2 for blob storage.

The `Vfs` trait now lives at `src/vfs.rs` (top-level). Desktop and
wasm both call `crate::vfs::with_vfs(...)` and get the right impl:
`OsVfs` (in `src/vfs.rs`, gated `#[cfg(feature = "desktop")]`) wraps
`std::fs::*`; `SnapshotVfs` (in `src/cf/snapshot_vfs.rs`, wasm only)
is the per-request preload from Workspace. Same split as the shell
port (`cloudflare_shell::FileSystem` trait + the
`cloudflare-shell-workspace` impl).

### Still to build

- **BusBridge** for `.bus sub` -- desktop uses thread + tokio runtime
  (gated); CF will use a Durable Object with WebSocket Hibernation.
  Both emit the same record stream.
- **Async eval refactor.** `Engine::run_closure` is sync. On Workers,
  long evals hold the isolate; SSE handlers have to yield. Probably
  means rewriting `src/worker.rs`'s eval loop (gated as desktop today)
  to an async variant for CF.
- **Storage primitives mapping** -- xs's `fjall` log + `cacache` CAS
  map ~1:1 to DO SQLite + R2 via `@cloudflare/shell`'s `Workspace`.
  The eventual `--store` story on CF reuses that. Lives in the xs
  repo, not here.

## Acknowledgements

The wasm path is well-trodden by the upstream Nushell team:

- [`nushell/nushell` `toolkit/wasm.nu`](https://github.com/nushell/nushell/blob/main/toolkit/wasm.nu)
  is the SSOT for which Nu crates compile to wasm32. They CI-check it
  with `cargo clippy --target wasm32-unknown-unknown
  --no-default-features -- -D warnings -D clippy::unwrap_used` per
  crate. We follow that list.
- [`@cptpiepmatz`](https://github.com/cptpiepmatz) drives upstream's
  wasm work; worth tracking before assuming a wasm gap is permanent.
- [`nu-on-web/nu-on-web`](https://github.com/nu-on-web/nu-on-web) ships
  Nushell in a browser today. Their Cargo recipe was the template for
  ours (`nu-command` with `default-features = false`,
  `features = ["js", "rand"]` + `getrandom/wasm_js` +
  `console_error_panic_hook`). Their `src/zenfs.rs` (local copy
  `.src/nu-on-web/src/zenfs.rs`) is the `wasm_bindgen` extern pattern
  for shadowing Nu's fs commands via a JS VFS backend; our `Vfs`
  follows the same pattern targeting `@cloudflare/shell` instead.
- [`@cloudflare/shell`](https://www.npmjs.com/package/@cloudflare/shell)
  -- Workers-native FS + `isomorphic-git`. Provides
  `WorkspaceFileSystem` (DO SQLite + R2) and `InMemoryFs`. Local copy
  `.src/agents/packages/shell/README.md`.

## Open questions

- The `desktop` feature cascade adds ~30 lines to `Cargo.toml` and ~10
  cfg gates inside `src/`. Acceptable as an upstream contribution, or
  do we keep the fork and not upstream the CF support at all?
- Curated Nu deps in `Cargo.toml` (`default-features = false` on
  `nu-protocol` etc.) -- safe to merge into upstream as a precondition
  for the wasm branch, or do upstream desktop builds rely on something
  we'd be stripping?
