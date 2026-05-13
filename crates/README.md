# `crates/` -- the cloudflare-shell family

Three folders live here, all built around one idea: **a filesystem
abstraction that runs on Cloudflare Workers, ported from
[`@cloudflare/shell@0.3.6`](https://www.npmjs.com/package/@cloudflare/shell).**

Once you have the abstraction, multiple things layer on top:
desktop-compatible types, a wasm-only DO+R2 backend, a worker that
exposes the abstraction over RPC, and so on. Each layer is its own
crate so consumers only pay for what they use.

## At a glance

| Folder | Kind | Target | What it is |
|---|---|---|---|
| [`cloudflare-shell/`](cloudflare-shell/) | library | any | The **interface** -- `FileSystem` trait + `Stat`/`EntryType`/`FsError` types + the generic conformance test suite. No `worker` dep at module scope. Compiles on desktop too. |
| [`cloudflare-shell-workspace/`](cloudflare-shell-workspace/) | library | wasm-only | One **implementation** of the interface: DO SQLite + R2 spillover, schema-compatible with the JS package. Plus a runner that drives the conformance suite against a real `Workspace` and returns a `worker::Response`. |
| [`cloudflare-shell-rpc/`](cloudflare-shell-rpc/) | sub-tree | wasm-only | A **deployable Worker** that exposes `cloudflare-shell-workspace` as a service-binding RPC (so other Workers can call it without HTTP). Self-contained subsystem: own server, client, types, demos, smoke, bench, deploy automation. |

## Dependency graph

```
cloudflare-shell  (trait + types + generic conformance suite)
        ▲
        │
        ├── cloudflare-shell-workspace  (DO SQLite + R2 impl + conformance runner)
        │           ▲
        │           ├── http-nu                          (server-side: per-user filesystem in src/cf/)
        │           └── cloudflare-shell-rpc/server      (exposes the FS as RPC + HTTP)
        │
        └── cloudflare-shell-rpc/server                  (uses the trait directly in some places)
                    ▲
                    ├── cloudflare-shell-rpc/types       (wire structs for the RPC surface)
                    ├── cloudflare-shell-rpc/client      (typed Rust client over the binding)
                    │           ▲
                    │           └── cloudflare-shell-rpc/demo-rust   (sample Worker)
                    └── cloudflare-shell-rpc/demo-js     (sample Worker, JS)
```

## Why two `cloudflare-shell-*` crates instead of one?

**The second implementation isn't hypothetical -- it's the upstream
JS one.** `@cloudflare/shell@0.3.6` ([cloudflare/agents
`packages/shell/`](https://github.com/cloudflare/agents/tree/main/packages/shell))
defines the `FileSystem` interface in `fs/interface.ts` and ships the
`Workspace` class in `filesystem.ts`. Our Rust port splits along the
same file boundary:

```
upstream (JS)                            this repo (Rust)
─────────────                            ────────────────
fs/interface.ts          ─port→          cloudflare-shell/src/interface.rs
fs/path-utils.ts         ─port→          cloudflare-shell/src/path_utils.rs
filesystem.ts (class)    ─port→          cloudflare-shell-workspace/src/filesystem.rs
```

So **two real implementations** of one interface coexist *today*:

1. **JS `Workspace`** (cloudflare/agents) -- runs in JS Workers.
2. **Rust `Workspace`** (this repo) -- runs in Rust Workers.

They share the same SQL table layout (`cf_workspace_<ns>`), same
column types + CHECK constraints, same R2 key shape
(`${prefix}/${ns}<path>`). A file written by one side is readable by
the other in the same Cloudflare account. The schema *is* the
interop contract; see
[`cloudflare-shell-workspace/PORT_STATUS.md`](cloudflare-shell-workspace/PORT_STATUS.md)
for the schema-compat table.

That bidirectional interop is the load-bearing reason the split is
real, not ceremony. The trait crate (`cloudflare-shell`) is the
Rust-side type vocabulary that describes the contract both sides
honour. If you flatten it into the impl crate, you can't talk about
the trait from desktop code, the conformance suite gets pinned to
wasm, and (more importantly) the fact that there's an *external*
implementation worth tracking gets buried.

Same shape as `std::io::Read` (trait) and `std::fs::File` (one impl)
-- with the twist that the "other impl" already exists in a different
language and we have to stay byte-compatible with it.

## Where each crate is consumed

- **`cloudflare-shell`** -- pulled by `cloudflare-shell-workspace`,
  `cloudflare-shell-rpc/server`, and http-nu's CF target
  (`src/cf/mod.rs`). Anywhere FS types appear.
- **`cloudflare-shell-workspace`** -- pulled by http-nu's CF target
  (per-user `Workspace` inside the `UserSpace` DO) and by
  `cloudflare-shell-rpc/server` (the DO that backs the RPC). http-nu
  serves `cloudflare_shell_workspace::run_conformance` at
  `GET /<user>/_workspace/conformance` as a self-test endpoint.
- **`cloudflare-shell-rpc/`** -- independent of http-nu. Deployable
  on its own as three Workers (server + two demos). See
  [its README](cloudflare-shell-rpc/README.md) for live URLs and the
  bench report.

## Conformance: suite vs runner

A perennial source of confusion (now documented in
[`cloudflare-shell-rpc/DECISIONS.md` D8](cloudflare-shell-rpc/DECISIONS.md)):

- `cloudflare-shell::conformance` -- the **suite**: generic
  `<F: FileSystem>` test functions.
- `cloudflare-shell-workspace::conformance_runner` -- the **runner**:
  drives the suite against a real `Workspace`, returns a
  `worker::Response`. wasm-only.

Two different jobs, same word in the name. Read both files
side-by-side if it ever stops being obvious.
