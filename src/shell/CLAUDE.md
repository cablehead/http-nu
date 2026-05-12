# `src/shell/` -- working rules

This directory is the **backend-agnostic** Rust port of
`@cloudflare/shell`. The whole reason it sits at top-level rather than
under `src/cf/` is so the `FileSystem` trait + `InMemoryFs` are
reachable from desktop -- without that, `cargo test` can't exercise
the port at all.

## ⚠️ The mock-divergence trap (READ FIRST)

`InMemoryFs` is a *behavioural double* for `Workspace`. The classic
failure mode of doubles is silent drift:

1. Someone tweaks `InMemoryFs::rm` to match what a test expects.
2. The test goes green. PR ships.
3. `Workspace::rm` (the real DO + R2 backend) behaves differently.
4. Production breaks. Tests don't catch it.

A passing test against a divergent mock is **worse than no test** --
it ships false confidence.

### Defence: write tests against the trait, run them against both impls

Tests that exercise FS behaviour belong in
[`conformance.rs`](conformance.rs), written generically against
`<F: FileSystem>`. Each conformance fn is then called twice:

1. **Against `InMemoryFs`** in `in_memory_fs.rs`'s `#[cfg(test)] mod tests`.
   Runs under `cargo test` on desktop. Fast feedback loop.
2. **Against `Workspace`** through the wasm harness at
   [`src/cf/conformance.rs`](../cf/conformance.rs), exposed as
   `GET /<user>/_workspace/conformance`. Invoke via:
   ```
   mise run cf:dev    # one terminal
   curl -i http://127.0.0.1:8787/alice/_workspace/conformance
   ```
   `200 OK` + `<n> passed` body means every assertion holds against
   the real DO SQLite + R2 backend. **A conformance change isn't
   complete until both legs (desktop `cargo test` AND this curl) come
   back green.**

If an assertion holds against the double but fails against the real
backend, the assertion is wrong, not the backend. Fix the conformance
test to express the *real* invariant; then make both impls satisfy it.

### What belongs in `conformance.rs`

Properties of the `FileSystem` contract -- the things a caller is
allowed to assume regardless of backend:

- `write_file(p, x)` then `read_file(p)` returns `Some(x)`.
- `stat` on missing path returns `Ok(None)`, not `Err`.
- `read_file_bytes` on a directory returns `Err(IsDir(_))`.
- `on_change` fires `Create` on first write, `Update` on second.
- ...

### What does NOT belong in `conformance.rs`

Backend-specific properties. Anti-examples:

- "Files >1.5MB spill to R2" -- Workspace-only.
- "State survives DO eviction" -- Workspace-only.
- "Lookups are O(1)" -- InMemoryFs-only.

Those tests live next to their impl -- in `in_memory_fs.rs` for the
double, in `src/cf/shell/filesystem.rs` (or a sibling wasm harness)
for Workspace.

## 0. Demand-driven port scope (READ FIRST when porting more upstream)

**Port from `@cloudflare/shell` only what http-nu actually calls.**
Upstream has plenty of surface we don't need (`backend.ts`,
`memory.ts`, `prompt.ts`, the Agents-SDK glue). The trap is feeling
like we should mirror the whole package; the discipline is to only
port a module when http-nu reaches for it.

Canonical evidence: the **File-level mapping** table in
[`src/cf/shell/PORT_STATUS.md`](../cf/shell/PORT_STATUS.md). Each row
is labelled "done", "TBD", or "skip" with a reason. Adding a port
means moving a row from TBD to done; "skip" rows shouldn't be ported
without first explaining why http-nu needs them.

Same principle holds for `src/cf/nu/nu_command/` (only shadow commands
that examples need; see
[`src/cf/nu/nu_command/CLAUDE.md`](../cf/nu/nu_command/CLAUDE.md)).

## 1. Provenance: every public item gets an `Upstream:` line

Every `pub fn`, `pub struct`, `pub enum`, `pub const` in this
directory starts its doc comment with one of:

```rust
/// Upstream: filesystem.ts:526 `readFile()`.
pub async fn read_file(...) { ... }

/// Port-only: <reason there is no upstream equivalent>.
pub fn ...
```

The `filename:line camelCaseName()` form is non-negotiable. The whole
point of the `src/shell/` + `src/cf/shell/` split is that a reviewer
who knows the upstream JS package can read both sides for real. Stale
line refs are fine and expected; deleted line refs aren't.

## 2. The split between `src/shell/` and `src/cf/shell/`

| Where                    | What lives there                                           | Reachable from desktop? |
|--------------------------|------------------------------------------------------------|-------------------------|
| `src/shell/`             | `FileSystem` trait, `Stat`, `EntryType`, all options,     | yes                     |
|                          | `WorkspaceChange*`, `FsError`, `path_utils`, `InMemoryFs`,|                         |
|                          | `conformance` suite                                        |                         |
| `src/cf/shell/`          | `Workspace` impl (uses `worker::SqlStorage` + `Bucket`),  | no (wasm-only via       |
|                          | `schema` (SQL DDL), `impl FileSystem for Workspace`       | `src/lib.rs:13` gate)   |

Files in `src/shell/` MUST NOT depend on the `worker` crate at module
scope. Internal helpers can use `worker::*` *inside a*
`#[cfg(all(feature = "cloudflare", target_arch = "wasm32"))]` block --
see `in_memory_fs.rs::now_secs` for the pattern. The desktop branch
provides the equivalent through `std`.

`FsError`'s `From<worker::Error>` impl in `error.rs` is gated the same
way, so the conversion only exists on wasm.

## 3. `Ok(None)` on ENOENT (deviation from upstream)

Upstream's `FileSystem` interface throws ENOENT. We return `Ok(None)`
on missing paths and reserve `Err(_)` for genuine errors (EISDIR,
ENOTDIR, ENOTEMPTY, EILSEQ, ENAMETOOLONG, ELOOP, EIO, ENOSPC,
NoSpace). Every impl of `FileSystem` MUST follow this -- otherwise
the conformance tests catch the drift.

This is the only intentional shape-level deviation from upstream's
interface. Documented in `interface.rs`'s trait doc.

## 4. POSIX error-prefix convention

`FsError`'s `Display` impl re-adds the POSIX prefix (`ENOENT:`,
`EISDIR:`, etc.) automatically. When constructing an error, pass only
the *detail* portion of the message:

```rust
// ✅ correct
return Err(FsError::NotFound(format!("rm {p} not found")));

// ❌ wrong -- the prefix is duplicated when Display formats
return Err(FsError::NotFound(format!("ENOENT: rm {p} not found")));
```

Match upstream's exact phrasing where it has a precedent (e.g.
`"cannot write to root directory"`, `"no such file or directory: <path>"`).

## 5. Conformance tests are mandatory for new FS surface

When you add a method to `FileSystem`, OR change a contract detail:

1. Add a conformance fn in `conformance.rs` that exercises the new
   behaviour.
2. Wire it into the `#[cfg(test)] mod tests` of `in_memory_fs.rs`.
3. Run it against `Workspace` via wrangler dev + curl (or, eventually,
   an automated wasm harness).
4. ONLY after step 3 passes, mark the work complete.

PRs that add a method without a conformance test are incomplete.

## 6. After every edit: both targets compile

```
cargo check                                                              # desktop
CF_HANDLER_PATH=../../examples/cf-workspace-browser/serve.nu \
  cargo check --target wasm32-unknown-unknown --features cloudflare --no-default-features
```

Plus `cargo test --lib shell` for the conformance suite. All three
have to be green.
