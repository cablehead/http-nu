# Cloudflare branch

## Mise patterns 
use MISE SSOT paterns for CI and local. Our .github repo is the pattern !! 

## Intro
Forward-looking notes for extending http-nu to run on Cloudflare. Nothing
here is implemented yet -- this file captures constraints, questions, and
candidate paths, scoped to what http-nu adds *on top of* xs.

The persistence layer is xs (cross.stream). See
`../xs/CLOUDFLARE.md` for the upstream story (DO-as-stream, R2-as-CAS,
auth, hybrid vs. native rewrite). Don't duplicate that here. Below is the
http-nu-specific delta.

## Goal

Run an http-nu deployment whose surface (HTTP, SSE/Datastar, the
"closure-as-handler" model, the embedded UI bus) is served from a
Cloudflare Worker, with state persisted in an xs instance that is itself
running on CF (DO + R2, per the xs doc). worker-rs is the obvious
target -- the existing code is Rust + tokio + hyper.

The local-first, single-binary deployment is non-negotiable and stays as
the default. CF support is additive.

## Ground rule: desktop must keep working

Every change for CF lands the Cargo way -- as optional dependencies and
feature flags, or in a sibling workspace crate. The desktop build is the
default; nothing is replaced in place. Concretely:

- `Cargo.toml` keeps today's deps as the default. CF-only deps
  (`worker`, `worker-build`-ish glue, anything wasm-bindgen) go in
  optional dependency blocks, gated by a `cloudflare` feature, and are
  not pulled in by `cargo build`.
- Today's heavy/incompatible deps (`ctrlc`, `notify`, `rustls`,
  `tokio-rustls`, `tower-http/fs`, `nu-cli`, `nu-command`,
  `nu-cmd-extra`, `nu-plugin-engine`) get moved behind a `desktop`
  feature in the default set. The CF target builds with
  `default-features = false, features = ["cloudflare"]`.
- A new workspace member (e.g. `http-nu-cf/`) holds the worker-rs
  entrypoint and the wasm-only glue. `crate-type = ["cdylib"]`. It
  depends on the existing `http-nu` crate as a library (which already
  exposes `lib.rs`) with `default-features = false`. The desktop
  binary's build path is unchanged.
- Modules in `src/` get cfg-gated where they touch capabilities the
  CF target doesn't have. Anything pure (request, response, bus,
  compression, content-type inference, the curated Nu commands) is
  reachable from both targets without cfg gymnastics.
- `cargo build` -- desktop, today's behavior.
  `cargo build -p http-nu-cf --target wasm32-unknown-unknown` (or
  `worker-build`) -- CF artifact. No new path leaks into the other.
- `mise.toml` gets `cf:*` tasks alongside today's tasks. The default
  `check` aggregator stays desktop-only; a separate `check:cf`
  runs wasm build + clippy on the CF crate.

If a CF need ever conflicts with desktop (e.g. an async-only eval
refactor in `worker.rs`), the resolution is: keep the desktop code
path intact behind `#[cfg(feature = "desktop")]` and add the async
variant behind `#[cfg(feature = "cloudflare")]`. Never break desktop
to make CF work.

## What http-nu is, today

A thin bridge between hyper and a Nushell closure (`{|req| ...}`),
plus an in-process bus, plus the xs commands (`.cat`, `.append`,
`.cas`). Every request:

1. ArcSwap-loads the current `Engine` (Nu state + bus + SSE cancel
   token) -- `src/handler.rs:48`.
2. Spawns an OS thread that runs the closure synchronously --
   `src/worker.rs:234`. This is the hot-path blocker.
3. Returns a Nu value whose type drives content negotiation
   (`src/worker.rs:69`). Streams pipe end-to-end with backpressure.

Hot reload is `notify` -> new `Engine` -> `ArcSwap::store` -> SSE cancel
token fires -> clients reconnect. `--topic <name>` loads the closure
*from xs* and live-reloads on appends -- this is the CF-friendly
deployment shape.

## The blocker: Nushell on `wasm32-unknown-unknown`

Spike-test this before anything else. Likely failures:

- `nu-cli` / `nu-command` pull in ctrlc, sysinfo, sqlite (rusqlite
  links libsqlite3 by default), filesystem and process commands. None
  of those compile to wasm32 unmodified.
- `nu-plugin-engine` spawns plugin subprocesses. Plugin support is
  already noted as drop-on-CF.
- `nu-engine` itself is mostly pure compute -- the question is whether
  we can build a curated context (drop `add_shell_command_context`,
  drop `add_cli_context`, drop `nu_cmd_extra`) and still get a usable
  language. `nu-cmd-lang`, `nu-parser`, `nu-protocol`, `nu-engine`,
  plus `nu-std`, plus our own `src/stdlib/*` are the only pieces the
  examples actually use.
- `std::thread::spawn` in `src/worker.rs` is the worst offender. wasm
  has no threads in the Workers runtime; eval has to become async.

Until the spike answers "yes, a curated Nu compiles to wasm32 and
evaluates a `{|req| ...}` closure with `await`-able primitives", every
path below is conditional. If the answer is no, http-nu on CF means
running a *different* scripting surface at the edge (rhai, mdjs, JS)
while keeping the routing/HTML/Datastar/xs concepts identical.

## Cloudflare Containers / Sandbox SDK: out

CF Containers are out (consistent with `../xs/CLOUDFLARE.md`). That
includes the Sandbox SDK, which is built on Containers + DO. CF
deployment means worker-rs targeting `wasm32-unknown-unknown` against
the Workers runtime, talking to DO + R2 + KV. No Linux processes at
the edge.

## A virtual filesystem trait, not per-call cfg gates

The cleanest way to keep desktop and CF on the same code path -- and
the answer to "what replaces the FS on Workers" -- is to introduce a
`Vfs` trait now and route every place that does filesystem IO through
it. Desktop wires it to `std::fs`; CF wires it to R2 (+ optionally KV
or DO storage for metadata/index). The rest of the code becomes
target-agnostic.

Surface (rough):

```rust
#[async_trait]
pub trait Vfs: Send + Sync {
    async fn read(&self, path: &str) -> Result<Bytes>;
    async fn metadata(&self, path: &str) -> Result<VfsMeta>;
    async fn read_dir(&self, path: &str) -> Result<Vec<VfsEntry>>;
    async fn exists(&self, path: &str) -> Result<bool>;
}

#[async_trait]
pub trait VfsWrite: Vfs {
    async fn write(&self, path: &str, body: Bytes) -> Result<()>;
    async fn remove(&self, path: &str) -> Result<()>;
}

#[async_trait]
pub trait VfsWatch: Vfs {
    fn watch(&self, path: &str) -> BoxStream<'static, VfsEvent>;
}
```

Three split traits because capabilities differ honestly: R2 has writes
but no native fs-watch; a read-only deployment may have `Vfs` only;
desktop has all three. Code that needs `.watch()` requires
`VfsWatch`; if the active impl doesn't supply it, `--watch` is a
build-time error on that target rather than a runtime surprise.

Implementations:

- `LocalFs` (default feature `desktop`) -- `tokio::fs` + `notify`.
  Today's behavior, no semantic change. This is what `cargo build`
  produces.
- `R2Fs` (feature `cloudflare`) -- backed by an R2 bucket binding via
  `worker::Bucket`. Lists, reads, conditional writes, deletes. No
  `watch`; CF deployments either skip `--watch` or get change events
  from elsewhere (DO push, xs append).
- `MemFs` (always available, used in tests) -- pure in-memory. Useful
  for the existing test suite and for the wasm tests where neither a
  bucket binding nor real fs is wanted.
- Optional later: `XsFs` -- read paths from an xs CAS by hash, list
  by topic. Lets `--topic` and `.static` share storage.

Call sites to migrate (incrementally; each one is independently
shippable on desktop without behavior change):

| Today | Move to Vfs |
|---|---|
| `tower_http::ServeDir` in `.static` | A small handler that does `vfs.read(path)`/`vfs.metadata(path)` and emits a 304/range-aware response |
| `std::fs::read_to_string` in `.mj file=...` (`src/commands.rs:913`) | `vfs.read(path).await` |
| `--store <path>` on desktop | Stays `std::path::Path` (xs is desktop-only there); on CF the store DO holds its own data and `Vfs` is only for non-store FS |
| `notify::recommended_watcher` for `--watch` (`src/main.rs:188`) | `VfsWatch::watch(...)` |
| Plugin loading from `Path` (`src/engine.rs:114`) | Permanently desktop-only; not migrated |

Prior art: `opendal` (Apache) already provides this exact abstraction
across local fs, S3, GCS, R2, HTTP, in-memory, etc., with async +
streaming. Worth evaluating before hand-rolling -- it would mean one
dep gives us `LocalFs` and `R2Fs` for free, including range reads
and listings. The shape above is roughly its `Operator` API. Decide
"adopt opendal" vs. "hand-rolled trait" early; it's load-bearing for
everything below.

Wiring: `AppConfig` carries a `vfs: Arc<dyn Vfs>` (and optionally
`Arc<dyn VfsWrite>`/`Arc<dyn VfsWatch>` when present). Built once at
startup from CLI flags on desktop and from `worker::Env` bindings on
CF. Handlers and Nu commands receive it through the existing engine
plumbing.

This makes the desktop/CF split *one* construction-time decision
instead of N cfg-guarded branches scattered through the code.

## Storage primitives: xs maps almost 1:1 to CF

xs's internal model and Cloudflare's storage bindings line up so
closely it's almost embarrassing. This is the strongest argument that
"http-nu (and xs) on CF" is a backend swap, not a redesign.

| xs primitive (today) | What it is | CF primitive |
|---|---|---|
| `fjall` LSM keyspace | append-only frame log keyed by `Scru128Id`, with topic secondary index | DO SQLite (per-stream DO holds the table) |
| `cacache` CAS | hash -> bytes, content-addressed | R2 (key = hash, body = bytes) |
| `Frame { id, topic, hash, ts, meta }` | row in the log | row in DO SQLite |
| `read(options { topic, follow, after })` | indexed scan + optional tailing | SQL query for the backlog + WS Hibernation for follow |
| `cas_read_sync(hash)` | blocking blob fetch | `r2.get(hash)` (async) |
| `cas_write(bytes) -> hash` | write-after-hash | compute hash, `r2.put(hash, bytes)` (no-op if exists) |
| `nu_modules_at(id)` | snapshot of VFS modules at a frame | indexed SQL query + R2 reads, cached in the DO |
| `--expose iroh://` | P2P transport | not portable; CF deployments are HTTP-only |
| processors (`actor`, `service`, `action`) | long-lived subscribers | DO with alarm-driven re-subscribe + Hibernation |

What this means for the abstractions:

- The `Vfs` trait above is **path-keyed**. It's the right thing for
  `.static`, `.mj file=...`, plugin paths.
- xs's primitives are different shape: **hash-keyed CAS** + **id-keyed
  log with topic index**. Don't shoehorn them through `Vfs`. They
  deserve their own trait pair, separate from `Vfs`:

```rust
#[async_trait]
pub trait Cas: Send + Sync {
    async fn read(&self, hash: &Hash) -> Result<Bytes>;
    async fn write(&self, bytes: Bytes) -> Result<Hash>;
    async fn exists(&self, hash: &Hash) -> Result<bool>;
}

#[async_trait]
pub trait FrameLog: Send + Sync {
    async fn append(&self, topic: &str, hash: Option<Hash>,
                    meta: Value) -> Result<Frame>;
    async fn read(&self, opts: ReadOptions) -> Result<BoxStream<'static, Frame>>;
    async fn read_one(&self, id: Scru128Id) -> Result<Option<Frame>>;
}
```

- Desktop impls: `FjallLog` + `CacacheCas` (today's xs, unchanged).
- CF impls: `DoSqliteLog` + `R2Cas`. Both fit native CF bindings
  without invention.
- KV is the odd one out -- not used by xs today. Fits well for tiny
  config-style values (e.g. last-seen frame id per subscriber, route
  metadata cache). Don't force xs through KV; it's lossy/eventual and
  the frame log needs strong consistency.

http-nu uses both abstractions:

- The handler closure (`--topic <name>` mode) is a CAS read of the
  latest frame's payload.
- `.static` reads an asset by path -- `Vfs`. On CF, `R2Fs` is just a
  thin wrapper over the same R2 bucket the CAS uses, but with
  path-keying instead of hash-keying.
- `.bus pub/sub` is a FrameLog with `meta = ephemeral`, in a Bus DO,
  with WS Hibernation for the subscribers. It does *not* need CAS or
  R2; payloads are small Nu values stored inline in DO SQLite (or just
  in memory if we keep the bus ephemeral).

Net effect: pick the right trait per call-site, then have two impls of
each (desktop + CF) with no business logic in between. The Cargo
feature flags select which impl is compiled in; the desktop binary
sees only `Fjall*`/`Cacache*`/`LocalFs` and never references any
worker-rs symbol.

## Candidate paths

1. **worker-rs + curated Nu engine + xs-on-CF** (native, ambitious).
   - Replace `tokio::main` + accept loop with a `#[event(fetch)]`
     handler that ArcSwap-loads the current engine and `.await`s a
     refactored eval. No threads.
   - Fetch the handler closure from a known xs topic at fetch time
     (or cache in DO storage on alarm).
   - Bus -> a per-deployment `Bus` Durable Object backed by DO SQLite
     (frame log) and Hibernation API WebSockets (subscribers). `.bus
     pub` becomes `stub.publish(...)`; `.bus sub` becomes a WS upgrade
     into the Bus DO.
   - `.static` -> R2; `.reverse-proxy` -> `fetch()` (Workers' fetch is
     subrequest-counted, watch limits).
   - `.mj` (templates) -- minijinja is pure Rust, should wasm cleanly.
   - `.md`, `.highlight` -- pulldown-cmark + syntect are pure Rust,
     should wasm cleanly. syntect uses lazy assets; check size.
   - SSE -- emit via `ReadableStream`/`TransformStream`. Brotli on
     small SSE frames is pure compute. The "drain-then-flush brotli
     stream" tuning (commits 1fda22b, 64b8e74) needs to survive the
     port; that is mostly compression code in `src/compression.rs`,
     not server code.
   - Datastar JS bundle (`src/handler.rs:27`) -- statically embedded
     today, stays embedded in the wasm artifact. Or move to R2 and
     short-circuit to it via cache. Either works.
   - This is largest re-implementation. It is also the only
     "http-nu on CF" that actually feels like http-nu.

2. **worker-rs as gateway, http-nu binary upstream** (hybrid).
   - Worker terminates TLS, authenticates, talks to a remote xs (or to
     an `http-nu serve --topic ...` instance) over HTTP. iroh transport
     stays self-hosted-only.
   - No Nu engine on CF. Nothing changes in this repo's hot path.
   - Pros: ships fast, gets edge + auth + cache.
   - Cons: same complaint as xs's hybrid -- not really "http-nu on
     CF" for someone whose only infra is a Cloudflare account.

3. **Surface-only port** (orthogonal).
   - Drop Nu at the edge. Re-implement the *concepts* (routing dispatch,
     HTML DSL, Datastar SSE helpers, content-type-from-value) in Rust
     for worker-rs, without an embedded scripting language. Handlers
     are Rust code.
   - Useful if path #1's Nu spike fails. Reuses the mental model and
     the xs-on-CF story but is no longer "Nushell-scriptable".
   - Effectively a sister project: "http-nu the framework" without the
     "nu" part on CF.

## Hot-path concerns specific to http-nu

| Concern | Today | On Workers |
|---|---|---|
| Per-request `std::thread::spawn` | `src/worker.rs:234` | Must become `async fn`; cancellation via `AbortSignal` / cancel token, not job kill |
| Engine ArcSwap | `src/main.rs:328` | Per-isolate; multi-isolate consistency comes from re-fetching from xs/DO on miss |
| `notify` watcher | `src/main.rs:188` | Gone. `--topic` from xs replaces it; xs DO emits "stream advanced" notifications via WS or alarm |
| `ctrlc`, signals | `src/main.rs:493` | Gone. Workers manage lifecycle |
| TLS / `aws_lc_rs` | `src/listener.rs:100` | Gone. Workers terminate TLS |
| TCP/UDS bind | `src/listener.rs` | Gone. fetch handler is the entrypoint |
| Plugins (`.so`) | `src/engine.rs:114` | Permanently dropped on CF builds |
| `.bus sub` thread+mini-runtime | `src/commands.rs:1777` | Replace with WS Hibernation in a Bus DO; `.bus sub` returns a stream backed by that WS |
| `.static` (tower-http ServeDir) | `src/commands.rs` | Replace with R2 binding; preserve `--fallback` semantics for SPAs |
| `.reverse-proxy` blocking read | `src/commands.rs:468` | Use `fetch()` with streaming body; subrequest limit applies |
| Logging threads + crossterm | `src/logging.rs:240` | Gone. Use `console_log` / Workers logger; jsonl mode maps to Workers' structured logs |

## The bus, specifically

The bus is what makes the 2048 / quotes / templates examples feel
alive. Its current implementation (`tokio::sync::broadcast` +
glob-matched subscribers) is in-process only -- a single-isolate
Worker would *almost* work, but Workers do not guarantee a single
isolate per route, so two clients can land on different isolates and
miss each other's events.

The right CF shape is a Durable Object per "bus namespace":

- `.bus pub topic value` -> Worker calls a `BusDO::publish(topic, value)`
  RPC. The DO appends to its SQLite (optional persistence; bus today is
  ephemeral, so we can keep that and only buffer for in-flight subs).
- `.bus sub pattern` -> Worker upgrades the request to a WebSocket
  paired with the BusDO. The DO uses the Hibernation API so idle subs
  cost nothing. Glob matching stays as it is in `src/bus.rs:67`.
- Lag policy stays: terminate the sub on overflow (`src/bus.rs:51`),
  let the client reconnect.

A BusDO is small, doesn't need R2, and is a useful primitive on its
own -- separable from xs.

## Streaming + Datastar

- `to sse` already produces a record stream the response path serializes
  as `text/event-stream`. On Workers this becomes a `TransformStream`
  whose writer is fed from the eval future. SSE works.
- The brotli streaming work (recent commits) is pure compression and
  should survive the port. Verify chunk-flush semantics under
  `TransformStream` -- it has its own backpressure model.
- Datastar JS bundle: keep embedded in the wasm artifact. Or, if size
  hurts, push to R2 and let CDN cache it. The route handler already
  short-circuits before Nu sees the request.

## Auth

Defer to `joeblew999/auth-service` and `joeblew999/authz-core`, same as
xs. http-nu has no auth today; on CF that is not a tenable default.
The Worker should authenticate at the edge before any DO/xs RPC.

## Tooling

- `mise.toml` already drives local dev. Add `[tools]` entries when CF
  work begins:
  - `"npm:wrangler"` -- deploy + secrets
  - `"cargo:worker-build"` -- builds the wasm artifact
  - keep `rust = "stable"` (worker-build wants a current toolchain)
- Tasks to add when wiring starts (not now):
  - `cf:dev` -> `wrangler dev`
  - `cf:build` -> `worker-build --release`
  - `cf:deploy` -> `wrangler deploy` (gated by env)
- CI: extend `.github/workflows/` only. No dagger. Reuse the
  `joeblew999/.github` reusable workflows for CF deploys (Doppler ->
  GH Actions -> Wrangler), same pattern as `plat-trunk` /
  `authz-core`.

## Open questions

1. Does a curated Nu (no `nu-cli`, no `nu-command` shell context, no
   plugins) compile to `wasm32-unknown-unknown`? **Spike this first.**
2. If yes, can `eval_block_with_early_return` be wrapped to yield to
   the JS event loop, or does it need to be run in a `worker_threads`-
   style helper? Workers don't have worker_threads; it has to yield.
3. Per-request CPU/duration limits on Workers (10ms-50ms baseline,
   higher with paid plans, 30s hard cap). Streaming SSE handlers run
   for as long as the client stays connected -- check whether SSE on
   Workers counts CPU continuously or only while writing. DO with
   Hibernation is the safer venue for long-lived streams.
4. xs's `--expose iroh://` is irrelevant on CF (no UDP). Confirm CF
   deployments are HTTP-only and document the loss of P2P.
5. Subrequest budget: `.reverse-proxy` and any handler that talks to
   the BusDO + xs DO + R2 + an upstream all spend subrequests. Audit
   typical handlers to see whether the 50-subrequest limit bites.
6. Hot reload semantics. On a single-binary deployment, `--topic`
   from xs gives near-instant reload via notify+ArcSwap. On CF,
   re-fetching the handler script per-isolate-cold-start is fine; the
   question is whether warm isolates need to be invalidated, and how
   (alarm-driven xs poll? push from xs DO?).

## Status

Not started. Sequencing:

1. Local mise + CI green on `joeblew999` branch (done).
2. Examples runnable from mise (done; hub works).
3. **Refactor `Cargo.toml` to feature-gate the heavy deps behind a
   `desktop` feature in the default set.** Verify `cargo build` and
   `mise run check` still pass unchanged. This is the precondition for
   every step below. No CF code yet.
4. xs/CLOUDFLARE.md path #1 (DO + R2) lands at least a spike, since
   http-nu's CF story sits on top of it.
5. Curated-Nu wasm spike, in a throwaway sibling crate. Outcome
   decides between path #1 and path #3 above.
6. Add `http-nu-cf/` workspace member with a trivial worker-rs `fetch`
   handler that depends on `http-nu` with `default-features = false`.
   Prove the split builds both ways.
7. Bus DO + WS Hibernation prototype. Useful regardless of which
   scripting surface wins.
8. Datastar JS bundle + `.static` (R2) + `to sse` end-to-end on a
   trivial handler.
9. The 2048 example as the integration test for the whole stack.
