# cloudflare-shell

> One of three crates in the `cloudflare-shell` family. For the
> family overview + why we split interface from impl, see
> [`../README.md`](../README.md).

Backend-agnostic Rust port of
[`@cloudflare/shell`](https://www.npmjs.com/package/@cloudflare/shell)'s
`FileSystem` abstraction.

```toml
[dependencies]
cloudflare-shell = "0.1"
# Or, to use a real impl:
cloudflare-shell-workspace = "0.1"  # DO SQLite + R2
```

## What you get

- **`FileSystem` trait** -- `async fn` interface over a filesystem.
  No impls in this crate; bring your own (or use
  [`cloudflare-shell-workspace`](https://docs.rs/cloudflare-shell-workspace)).
- **Shared types** -- `Stat`, `EntryType`, options structs,
  `WorkspaceChangeEvent` for filesystem listeners, POSIX-style mode
  constants.
- **`FsError`** -- typed error enum with POSIX-prefixed `Display`
  (`ENOENT:`, `EISDIR:`, ...). Optional `From<worker::Error>` on the
  `workers` feature.
- **`path_utils`** -- `normalize`, `parent_path`, `path_name`,
  `normalize_path` (validates length). Mirrors upstream's
  `fs/path-utils.ts`.
- **`conformance`** -- generic `<F: FileSystem>` test functions you
  can run against your impl in a wasm integration test.

## Layout

```
src/
  lib.rs              module entry + re-exports
  interface.rs        FileSystem trait + Stat / EntryType / options /
                      WorkspaceChange* / constants. Mirrors upstream
                      fs/interface.ts plus the type exports at the
                      top of filesystem.ts.
  error.rs            FsError enum + Result alias. POSIX-prefixed
                      Display. From<worker::Error> behind `workers`
                      feature.
  path_utils.rs       normalize / normalize_path / parent_path /
                      path_name. Mirrors upstream fs/path-utils.ts.
  conformance.rs      Generic `<F: FileSystem>` test functions.
```

## Running conformance against your impl

The conformance suite is a set of `async fn` test functions. Drive
them from your own integration harness (wasm-target test, Worker
endpoint, etc.):

```rust
use cloudflare_shell::conformance as suite;

// Each fn assumes a fresh filesystem; call `wipe_root` between if you
// reuse one instance.
suite::round_trip(&fs).await;
suite::enoent_returns_ok_none(&fs).await;
suite::eisdir_on_read_of_directory(&fs).await;
// ...

// The on_change conformance fn needs a listener-setter because
// `set_on_change` isn't on the FileSystem trait:
suite::on_change_emits_create_then_update_then_delete(&fs, |fs, cb| {
    fs.set_on_change(cb);
}).await;
```

## Upstream provenance

Tracked in
[`crates/cloudflare-shell-workspace/PORT_STATUS.md`](../cloudflare-shell-workspace/PORT_STATUS.md).
Every `pub` item carries an `Upstream:` line citing the TS file +
line.

## License

MIT.
