# Design decisions

Durable design rationale for `cloudflare-shell-rpc`. Each entry has a
**Why**, **Trade-off**, and **Revisit when** so future contributors
know which choices are load-bearing and which are open.

---

## D1. Custom `shim.js` (worker-build CUSTOM_SHIM mode)

**Decision:** The server's WorkerEntrypoint is a hand-written
`shim.js` template loaded via `worker-build`'s `CUSTOM_SHIM` env var,
not the default auto-generated shim.

**Why:** worker-rs 0.8's auto-generated shim emits
`Entrypoint.prototype.X = imports.X;` for non-fetch `#[wasm_bindgen]`
exports. Those bare assignments don't inject `this.env`, so the wasm
side can't reach DurableObject / R2 / KV bindings. Our RPC methods
must reach the DO; bindings need env.

**Trade-off:** Hand-rolling means the JS and Rust sides can drift
silently. We have a `build.rs` guard that asserts each
`#[wasm_bindgen(js_name = X)]` has a matching `async X(args)` in
shim.js and `exit(1)`s on drift (build fails). Cost: one more file to
maintain.

**Revisit when:** worker-rs releases a version that uses
`wasm-bindgen` PR
[#4757](https://github.com/rustwasm/wasm-bindgen/pull/4757)
("classless this type", **merged upstream 2025-10-28**, ships in
wasm-bindgen-cli 0.2.100+). As of worker-rs v0.8.3,
`worker-build/src/main.rs:191` still carries the literal TODO
"Switch these over to PR 4757" -- but the workers-rs team is
**actively coordinating with the wasm-bindgen team** on the rollout,
not stuck. Two pieces need to land in worker-rs before our shim can
go: (1) flip the existing fetch/queue/scheduled wrapper to use the
new feature, (2) extend env injection to arbitrary `#[wasm_bindgen]`
exports so consumer Workers' RPC methods get this.env. Neither
needs an issue from us; just watch the worker-rs changelog. When the
release lands: delete `shim.js`, delete `build.rs`, remove
`CUSTOM_SHIM` from the `cf:fs:build` mise task, point
`wrangler.toml`'s `main` at worker-build's default output, re-run
smoke + bench, confirm zero behavior change.

---

## D2. Base64-encoded bytes on the wire

**Decision:** File contents travel as base64-encoded `String` in the
RPC request/response JSON, not as raw `Vec<u8>` / `Uint8Array`.

**Why:**

1. **JS consumer friendliness.** A JS Worker passing
   `{ data: <Uint8Array> }` across the service-binding boundary is
   technically supported by Cloudflare's structured-clone protocol,
   but the typical JS use case (`fetch().arrayBuffer()`, parse with
   `TextDecoder`, etc.) keeps the byte handling on the consumer side.
   Base64 keeps the wire JSON-pure -- no special handling for binary.
2. **serde-wasm-bindgen quirk.** By default `serde_wasm_bindgen`
   serializes `Vec<u8>` as a JS *array of numbers* (not Uint8Array),
   which is ~10x slower than the equivalent base64 string for any
   meaningful payload size. The `serde_bytes` wrapper avoids this,
   but only on the Rust client side -- JS callers would still see the
   number-array shape unless we hand-write JS converters.
3. **HTTP route parity.** The server's HTTP routes also speak this
   wire format (via `Authorization: Bearer` auth). Base64 strings
   round-trip through HTTP transparently. Binary would require a
   different envelope (multipart or raw body) for the HTTP path.

**Trade-off:** ~33% size overhead vs. raw bytes. Encode/decode CPU
cost on both sides (small but non-zero). For file payloads up to a
few MB this is fine; for video / large datasets this adds up.

**Revisit when:** A real workload starts pushing >1 MB files through
the RPC and profiling shows base64 in the hot path. Migration path:
add `Vec<u8>` fields wrapped in `serde_bytes::Bytes` *alongside* the
`String` fields, version the wire, give consumers time to migrate.

---

## D3. Double-JSON entrypoint <-> DO boundary

**Decision:** The entrypoint serializes args as JSON, sends as the
body of an internal `Stub::fetch_with_request`. The DO deserializes
that JSON back into the same Rust struct. Two encode/decode passes
per RPC.

**Why:** worker-rs 0.8 `#[durable_object]` exposes only `fetch(req)`
-- there are no typed DO RPC methods. The only way to send typed args
across the entrypoint -> DO boundary is to put them in an HTTP-shaped
request body. JSON is the lowest-friction format for that, and the
internal-fetch URL (`/read_file`, `/stat`, etc.) acts as the method
dispatcher.

**Trade-off:** Two extra serializations per RPC vs. zero in a typed
DO RPC world. Cost is bounded -- request structs are small
(~100-300 bytes), so the JSON pass is a few microseconds. Visible in
the bench's `js_vs_server_pct` column (the binding hop, which
includes this round-trip).

**Revisit when:** worker-rs grows typed DO RPC, OR if the bench
shows binding-hop overhead becoming load-bearing (>40% on small
ops). Faster fallback: switch the internal body to `postcard` or
`rmp_serde`. Same dispatch shape, denser bytes, ~2-3x faster
serialization. Drop-in if we measure it matters.

---

## D4. Internal-fetch dispatcher (entrypoint -> DO)

**Decision:** The entrypoint sends to the DO as
`POST https://shell-fs-rpc/<method>` with the JSON body. The URL path
is the dispatch key inside `do_obj::fetch`. The hostname is irrelevant
because the DO never observes it -- DO stubs route by ID, not host.

**Why:** Cloudflare DOs only expose a single `fetch(req)` entry --
there's no typed RPC into them today. Path-based dispatch is the
standard pattern (used by every JS DO codebase). Choosing `POST`
universally so the body always carries the args is the simplest
shape. (`GET` with a JSON query string would be uglier and limited by
URL length.)

**Trade-off:** Adding a new RPC method means: (a) the
`#[wasm_bindgen]` export in `rpc.rs`, (b) the shim.js mirror, (c) the
DO route + handler in `do_obj.rs`, (d) the type in
`cloudflare-shell-rpc-types`. Four touchpoints; each has a guardrail
(build.rs catches a-b drift; type system catches c-d drift).

**Revisit when:** Typed DO RPC arrives. Then the entrypoint can
`stub.read_file(&args).await?` directly and `do_obj`'s URL-path
match drops out.

---

## D5. Per-DO `Workspace` cache via `RefCell<HashMap>`

**Decision:** `ShellFsDo` holds `RefCell<HashMap<String, Rc<Workspace>>>`,
keyed by namespace. `open_workspace` consults the cache and only
constructs a fresh `Workspace` (running `bootstrap()` -- the
`CREATE TABLE IF NOT EXISTS` + `CREATE INDEX` + count-root + maybe
`INSERT` root row -- on cache miss).

**Why:** Without caching, every RPC call ran four idempotent SQL
execs as warm-up. Cached, that runs exactly once per (DO instance,
namespace) lifetime. Workers DOs are single-threaded by construction,
so `RefCell` is safe; no `Mutex` needed.

**Trade-off:** Adds ~3 lines of state to `ShellFsDo`. Memory is one
`Workspace` per namespace per DO instance; each is cheap (`SqlStorage`
is Clone, `Bucket` is Clone). The cache lives for the isolate's
lifetime; eviction by Workers runtime drops it cleanly.

**Revisit when:** Never, probably. The cache is invariant-true: a
`Workspace` constructed for namespace N at time T is functionally
identical to one constructed at time T+1 because the underlying
storage IS the same (DO SQLite + R2 are durable).

---

## D6. Hyphen-banning `VALID_NAMESPACE` regex

**Decision:** Namespaces must match `/^[a-zA-Z][a-zA-Z0-9_]*$/`.
Mirrors `@cloudflare/shell@0.3.6`'s `VALID_NAMESPACE`. Enforced in
`Workspace::new` -- the lowest layer -- so http-nu's existing usage,
`cloudflare-shell-rpc`'s RPC, and the server's HTTP routes all
inherit the check.

**Why:** The namespace string flows into raw SQL DDL/queries via
`format!("cf_workspace_{ns}")` (table-name interpolation). SqlStorage
parameterizes column values (`?`) but cannot parameterize table
names. Without validation, a namespace of
`x; DROP TABLE cf_workspace_default; --` would break out of the
table-name position. Validation is the only line of defense.

**Trade-off:** Hyphens, dots, slashes, non-ASCII all rejected. If a
caller "owns" a hyphenated user_id elsewhere in the system, they
have to map to an underscore form (`alice-bob` -> `alice_bob`) before
talking to this server. Documented in
`server/README.md` + the smoke test's `bad-namespace rejection` step.

**Revisit when:** Upstream `@cloudflare/shell` relaxes the regex. We
mirror upstream by design (cross-language interop with their JS
package); if they widen, we widen.

---

## D7. Opt-in shared-secret token, not always-on auth

**Decision:** The server reads `SHELL_FS_TOKEN` env var. If set,
every RPC + HTTP request must carry a matching token (`auth` field
for RPC, `Authorization: Bearer <token>` for HTTP). If unset, no
auth check runs.

**Why:** Cloudflare service bindings already authenticate at the
account level -- only Workers in the same account can bind. For
single-account deployments the binding boundary IS the auth boundary;
no token needed. For multi-tenant / cross-account /
publicly-reachable HTTP deploys, the token closes the gap. Making it
opt-in keeps the dev/demo path zero-config.

**Trade-off:** Operators must explicitly turn auth on for production
deploys that expose the server's `workers.dev` URL. README documents
this. Plain string compare (no constant-time `subtle::CtCompare`
available to wasm in worker-rs), but tokens aren't user-controlled
so timing-oracle isn't a credible threat.

**Revisit when:** A use case wants per-binding namespace allowlists
(consumer X can only touch namespaces matching pattern Y) -- the
plumbing is `env.var("SHELL_FS_*")` parsing in `rpc.rs`/`http.rs`
with no fundamental redesign. Or when Cloudflare exposes caller
identity through the RPC fabric, at which point token-based auth
becomes redundant for in-account callers.

---

## D8. Conformance "suite" vs "runner" split

**Decision:** Two files named *conformance* something live in two
different crates:

| Crate | File | Role |
|---|---|---|
| `cloudflare-shell` | `src/conformance.rs` | the **suite** -- generic `<F: FileSystem>` test functions. Pure, backend-agnostic. |
| `cloudflare-shell-workspace` | `src/conformance_runner.rs` | the **runner** -- constructs a real `Workspace`, calls each suite function against it, returns a `worker::Response`. wasm-only. |

**Why:** the trait contract is testable independently of any backend
(the suite). Running those tests against a real DO + R2 backend
requires wasm, a `Workspace`, and a `worker::Response` shape (the
runner). Splitting along the crate boundary keeps the suite reusable
for any future `FileSystem` impl, while the runner stays close to the
impl it drives.

**Trade-off:** Two modules whose names share the word *conformance*
are easy to confuse. The runner used to be named `conformance.rs`
which made the suite-vs-runner distinction invisible. Renamed to
`conformance_runner.rs`; the public function stays
`cloudflare_shell_workspace::run_conformance` (called by http-nu's
`/_workspace/conformance` route, no breaking change).

**Revisit when:** A second `FileSystem` impl arrives that wants its
own runner. At that point either the runner shape itself becomes a
reusable abstraction (extract into a third "runner-trait" module) or
each impl ships its own `*_conformance_runner.rs` -- both are fine.

---

## Out of scope (intentional non-decisions)

- **WIT codegen for the Rust client.** The hand-written `extern "C"`
  block in `client/src/sys.rs` is mechanical but stable; WIT's
  codegen is pre-alpha and only handles primitives. Revisit only when
  WIT codegen handles structured types end-to-end.
- **Streaming reads/writes.** All FS ops are buffered (read returns
  full bytes, write takes full bytes). For files >5MB this isn't
  ideal but isn't worth the complexity until a real workload pushes
  there.
- **R2 prefix override.** `Workspace::r2_prefix` defaults to the
  namespace. Upstream `@cloudflare/shell` lets you override; we
  don't, because no caller has asked for it yet.
