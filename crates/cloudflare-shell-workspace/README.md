# cloudflare-shell-workspace

> One of three crates in the `cloudflare-shell` family. For the
> family overview + why we split interface from impl, see
> [`../README.md`](../README.md).

DurableObject + R2 implementation of
[`cloudflare-shell`](../cloudflare-shell/)'s `FileSystem` trait.
Schema-compatible with
[`@cloudflare/shell@0.3.6`](https://www.npmjs.com/package/@cloudflare/shell):
data written from either side is readable by the other.

```toml
[dependencies]
cloudflare-shell-workspace = "0.1"
```

This crate is **wasm-only** (depends on `worker = "0.8"`
unconditionally). Use it from inside a Workers Rust project.

## What you get

- **`Workspace`** -- a per-user filesystem backed by a DurableObject's
  SQLite storage (files <= 1.5 MB stay inline as `BLOB` rows) plus
  R2 (files > 1.5 MB spill, keyed under `${prefix}/${namespace}<path>`).
- **`DEFAULT_NAMESPACE`** -- the default `cf_workspace_<ns>` table
  suffix when you don't pass one.
- Re-exports of `cloudflare_shell::{FileSystem, Stat, EntryType,
  FsError, ...}` so callers don't need a second `use` statement.

## Quick start

```rust
use worker::*;
use cloudflare_shell_workspace::Workspace;

#[durable_object]
pub struct MyDO { state: State, env: Env }

#[durable_object]
impl DurableObject for MyDO {
    fn new(state: State, env: Env) -> Self { Self { state, env } }

    async fn fetch(&mut self, _req: Request) -> Result<Response> {
        let sql = self.state.storage().sql();
        let r2  = self.env.bucket("WORKSPACE_FILES").ok();
        let ws  = Workspace::default(sql, r2)?;

        ws.write_file("/hello.txt", "world", None).await?;
        let body = ws.read_file("/hello.txt").await?.unwrap_or_default();
        Response::ok(body)
    }
}
```

## Schema (the interop contract)

- One table per namespace: `cf_workspace_<namespace>`.
- Columns + CHECK constraints + R2 key shape are byte-identical with
  upstream. See [`PORT_STATUS.md`](PORT_STATUS.md)'s "Schema
  compatibility" section.
- Inline threshold is 1.5 MB (`1_500_000` bytes). Above that, the row
  carries `storage_backend = 'r2'` and the bytes live in R2 under
  `${r2_prefix}/${namespace}<path>`.

If you change anything that touches table shape or key shape, treat
it as a versioning event. See [`CLAUDE.md`](CLAUDE.md) rule 3.

## Layout

Filenames mirror upstream `@cloudflare/shell` path-for-path so a
reviewer who knows the JS package can read both sides together.

```
src/
  lib.rs              module entry + re-exports
  filesystem.rs       <- filesystem.ts (the Workspace class)
  schema.rs           SQL DDL (extracted; inline in filesystem.ts upstream)
```

## Reading the code

Every public item starts with a doc comment that pins it to the
upstream source:

```rust
/// Upstream: filesystem.ts:526 `readFile()`.
pub async fn read_file(&self, path: &str) -> Result<Option<String>> { ... }

/// Port-only: TS resolves symlinks inline inside `stat` / `readFile`;
/// we surface a public helper because callers want the resolved path
/// directly.
pub async fn realpath(&self, path: &str) -> Result<Option<String>> { ... }
```

Open the upstream TS file at the cited line and diff the bodies.
That's the side-by-side review experience.

## Conformance

Run [`cloudflare_shell::conformance`](https://docs.rs/cloudflare-shell)
against a real `Workspace` from a Workers route -- that's the canonical
parity check. http-nu wires this up at
`GET /<user>/_workspace/conformance`; the pattern is reusable from any
Workers Rust project.

## Status

Tracked in [`PORT_STATUS.md`](PORT_STATUS.md) -- which upstream
files / methods are ported, schema-compat assertions, behavioural
parity items, ranked next port targets.

Upstream tracking issue:
[cloudflare/workers-rs#998](https://github.com/cloudflare/workers-rs/issues/998).

## License

MIT.
