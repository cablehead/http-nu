# `cloudflare-shell` -- working rules

Backend-agnostic Rust port of
[`@cloudflare/shell`](https://www.npmjs.com/package/@cloudflare/shell)'s
`FileSystem` abstraction: trait, types, error enum, path utilities,
generic conformance suite. Pair with an impl (today:
[`cloudflare-shell-workspace`](../cloudflare-shell-workspace/) for
DO SQLite + R2) to get a working filesystem.

This crate is meant to be **reusable from any Workers (or non-Workers)
Rust project**. Every rule below exists to keep that property.

## 0. Keep it reusable

- **No project-specific types.** Nothing http-nu-shaped, nothing
  workers-rs-specific at module scope.
- **`worker::*` is gated behind the `workers` feature.** That feature
  enables `From<worker::Error>` / `From<FsError> for worker::Error` so
  impls that bridge worker-rs error boundaries don't have to. Anything
  more `worker`-specific than that doesn't belong here -- it belongs
  next to the impl that needs it.
- **No desktop-only conveniences.** The whole point is to compile on
  any target Rust supports.

## 1. Demand-driven port scope

Port from `@cloudflare/shell` only what existing consumers actually
call. Upstream has surface we don't need (`backend.ts`, `memory.ts`,
`prompt.ts`, the Agents-SDK glue); the discipline is to only port a
module when a real consumer reaches for it.

Adjacent ledger:
[`crates/cloudflare-shell-workspace/PORT_STATUS.md`](../cloudflare-shell-workspace/PORT_STATUS.md)
tracks the running coverage.

## 2. Provenance: every public item gets an `Upstream:` line

Every `pub fn`, `pub struct`, `pub enum`, `pub const` in this crate
starts its doc comment with one of:

```rust
/// Upstream: filesystem.ts:526 `readFile()`.
pub fn ...

/// Port-only: <reason there is no upstream equivalent>.
pub fn ...
```

The `filename:line camelCaseName()` form is non-negotiable. A reviewer
who knows the upstream JS package needs to be able to read both sides
for real. Stale line refs are fine and expected; deleted line refs
aren't.

## 3. `Ok(None)` on ENOENT (deviation from upstream)

Upstream's `FileSystem` interface throws ENOENT. We return `Ok(None)`
on missing paths and reserve `Err(_)` for genuine errors (EISDIR,
ENOTDIR, ENOTEMPTY, EILSEQ, ENAMETOOLONG, ELOOP, EIO, ENOSPC,
NoSpace). Every impl of `FileSystem` MUST follow this -- the
conformance suite catches the drift.

This is the only intentional shape-level deviation from upstream's
interface. Documented in `interface.rs`'s trait doc.

## 4. POSIX error-prefix convention

`FsError`'s `Display` impl re-adds the POSIX prefix (`ENOENT:`,
`EISDIR:`, etc.) automatically. When constructing an error, pass only
the *detail* portion of the message:

```rust
// correct
return Err(FsError::NotFound(format!("rm {p} not found")));

// wrong -- the prefix is duplicated when Display formats
return Err(FsError::NotFound(format!("ENOENT: rm {p} not found")));
```

Match upstream's exact phrasing where it has a precedent (e.g.
`"cannot write to root directory"`, `"no such file or directory: <path>"`).

## 5. Conformance suite -- what belongs

`conformance.rs` functions are generic over `<F: FileSystem>` and
express properties of the trait contract a caller can rely on
regardless of backend:

- `write_file(p, x)` then `read_file(p)` returns `Some(x)`.
- `stat` on missing path returns `Ok(None)`, not `Err`.
- `read_file_bytes` on a directory returns `Err(IsDir(_))`.
- `on_change` fires `Create` on first write, `Update` on second.

What does NOT belong: backend-specific properties. "Files > 1.5MB
spill to R2" or "state survives DO eviction" are Workspace-only and
live in the workspace crate.

## 6. No desktop test double

We deliberately don't ship an in-memory `FileSystem` impl just to make
desktop tests fast. A double that nobody depends on at runtime is just
code that drifts from real impls. Run conformance against a real impl
(e.g. `cloudflare-shell-workspace::Workspace` in a wasm integration
test). If a second `FileSystem` impl becomes useful later, the
generic conformance functions are ready for it.

## 7. Conformance is mandatory for new FS surface

When you add a method to `FileSystem`, OR change a contract detail:

1. Add a conformance fn in `conformance.rs` that exercises the new
   behaviour.
2. Wire it into every impl's integration harness.
3. ONLY after step 2 passes, mark the work complete.

PRs that add a method without a conformance test are incomplete.

## 8. After every edit: the crate compiles standalone

```
cargo build -p cloudflare-shell                    # no features
cargo build -p cloudflare-shell --features workers # workers integration
cargo test  -p cloudflare-shell
```

All three have to be green.
