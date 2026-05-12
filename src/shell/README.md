# `src/shell/` -- backend-agnostic FS layer

Rust port of [`@cloudflare/shell`](https://www.npmjs.com/package/@cloudflare/shell)'s
abstract layer (`fs/interface.ts`, `fs/in-memory-fs.ts`, `fs/path-utils.ts`)
plus a typed error enum, a conformance test suite, and (later) any
other backend-agnostic helpers.

This module sits at top level rather than under `src/cf/` because it
must be reachable from desktop. The wasm-only `Workspace` impl
(DO SQLite + R2) lives under [`src/cf/shell/`](../cf/shell/) and
implements `crate::shell::FileSystem` so callers can be polymorphic.

## Layout

```
src/shell/
  README.md           you are here
  CLAUDE.md           working rules + the mock-divergence warning
  mod.rs              module entry + re-exports

  interface.rs        FileSystem trait + Stat / EntryType / options /
                      WorkspaceChange* / constants. Mirrors upstream
                      fs/interface.ts plus the type exports at the
                      top of filesystem.ts.
  error.rs            FsError enum + Result alias. POSIX-prefixed
                      Display. From<worker::Error> gated to wasm.
  in_memory_fs.rs     Pure-Rust InMemoryFs impl. Mirrors upstream
                      fs/in-memory-fs.ts.
  path_utils.rs       normalize / normalize_path / parent_path /
                      path_name. Mirrors upstream fs/path-utils.ts.
  conformance.rs      Generic FileSystem tests. Run against EVERY
                      impl. The keystone for the mock-divergence
                      defence.
```

## The two impls today

| Impl                                | Where                             | Backend                  |
|-------------------------------------|-----------------------------------|--------------------------|
| `crate::shell::InMemoryFs`          | `src/shell/in_memory_fs.rs`       | `HashMap<String, Node>`  |
| `crate::cf::shell::Workspace`       | `src/cf/shell/filesystem.rs`      | DO SQLite + R2 spill     |

Both `impl crate::shell::FileSystem`. Test code that's parameterised
over `<F: FileSystem>` runs against either backend. The
[`conformance`](conformance.rs) module is exactly this.

## The mock-divergence warning

`InMemoryFs` is a *behavioural double*. A test passing against
`InMemoryFs` but not against `Workspace` is a real bug -- almost
always in the test, not the backend. The defence is to write tests
generically and run them against both. **Do not** add InMemoryFs-only
tests that try to verify storage semantics; those belong against
`Workspace` via wrangler dev.

See [`CLAUDE.md`](CLAUDE.md) for the full discipline.

## Running

```bash
# Desktop conformance tests (run on every commit):
cargo test --lib shell

# Wasm build (Workspace impl):
CF_HANDLER_PATH=../../examples/cf-workspace-browser/serve.nu \
  cargo check --target wasm32-unknown-unknown --features cloudflare --no-default-features

# End-to-end conformance against Workspace (manual today):
mise run cf:dev
# ... and curl the _workspace/ debug routes to mirror the conformance
#     assertions. Automating this loop is future work.
```

## Upstream coverage

The running ledger lives in
[`src/cf/shell/PORT_STATUS.md`](../cf/shell/PORT_STATUS.md). When a
new method or type is ported into `src/shell/`, update that doc.
