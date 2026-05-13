# Cross-stream (xs) shadows for CF -- port plan

xs's Nu integration ports to wasm by mirroring its `src/nu/` layout
path-for-path here. Same discipline as our `src/cf/nu/nu_command/` <->
`nu-command/src/` mirror and our `crates/cloudflare-shell*/` <->
`@cloudflare/shell` mirror. A reviewer who knows xs can read both
sides for real.

Upstream xs (read-only reference, never built for wasm):
`/Users/apple/workspace/go/src/github.com/joeblew999/xs/src/nu/`.

## Layout (path-for-path with xs)

```
xs/src/nu/                          src/cf/nu/xs/
  mod.rs                             mod.rs
  engine.rs                          (omitted -- we use our own engine)
  vfs.rs                             vfs.rs
  util.rs                            util.rs
  config.rs                          (port if a demo needs it)
  commands/                          commands/
    mod.rs                             mod.rs
    append_command.rs                  append.rs
    append_command_buffered.rs         (later)
    cas_command.rs                     (later)
    cat_command.rs                     cat.rs
    cat_stream_command.rs              cat_stream.rs
    get_command.rs                     (later)
    last_command.rs                    last.rs
    last_stream_command.rs             last_stream.rs
    remove_command.rs                  (later)
    scru128_command.rs                 (later)
```

xs filenames end in `_command.rs`; we drop the suffix because
`src/cf/nu/xs/commands/` already names the kind. Mirror the upstream
file name + line refs in module docs so audits stay mechanical (same
rule as the other ports).

## Architectural commitment: xs frames ride on Workspace

xs upstream stores frames in `fjall` (LSM kv) + `cacache` (CAS blob).
Neither compiles on wasm. We replace both with our existing
`cloudflare-shell-workspace::Workspace` (DO SQLite + R2 spill).

Frame storage on CF:
- `/.xs/<topic>/<scru128>.json` -- one file per frame.
- File body = JSON-encoded Frame (id, topic, meta, hash, ttl, content
  ref). Big payloads R2-spill via Workspace's existing threshold.
- `.cat <topic>` = `read_dir("/.xs/<topic>")` + sort by name (scru128
  is monotonic, name-sort = chronological).
- `.last <topic>` = last entry of the above.
- `.last <topic> --follow` = register a Workspace `on_change`
  callback filtered to `/.xs/<topic>/*`; stream new frames on Create.

This means **one persistent storage primitive on CF** (Workspace) for
everything file-shaped, including xs frames. No parallel SQL table,
no separate R2 prefix, no migration story. Stor is the only other
primitive (SQL passthrough -- different access pattern -- separate
track).

## Why xs upstream stays untouched

- xs's Nu commands take a `Store` reference. We mirror the surface
  with a `store.rs` exposing a `Store` struct with the same method
  names, Workspace-backed.
- The COMMANDS themselves port nearly verbatim. The biggest line
  diffs are `use` paths and the `Store` impl details.
- xs upstream stays a pure desktop dependency. We never build it for
  wasm. No fork, no feature flag in xs.

## The `Store` shim

`src/cf/nu/xs/store.rs` exposes:

```rust
pub struct Store { /* holds a Workspace handle */ }

impl Store {
    pub fn append(&self, frame: Frame) -> Result<Frame, Error>;
    pub fn read_sync(&self, opts: ReadOptions) -> impl Iterator<Item = Frame>;
    pub fn last(&self, topic: &str) -> Result<Option<Frame>, Error>;
    // ...
}
```

Same method names as xs's `Store`. Internally writes/reads
`/.xs/<topic>/<scru128>.json` via Workspace. CAS-by-hash semantics
are simulated by content-addressed paths (`/.xs/cas/<hash>`).

Pin the upstream xs revision in `store.rs` module docs so future
audits know the schema we mirrored.

## The Nu `VirtualPath` story (xs's `vfs.rs`)

xs's `src/nu/vfs.rs` registers stored Nu modules in `EngineState`'s
virtual path table so scripts can write `use foo/bar` without disk.
Each entry with topic `discord.api.nu` becomes `discord/api/mod.nu`
in Nu's virtual fs.

Direct mirror works on CF: at engine init, iterate over
`/.xs/modules/<topic>.nu` files in Workspace, call
`working_set.add_virtual_path(...)`, merge_delta. Nu's parser then
resolves `use` natively.

**Open question worth investigating BEFORE the templates port:** does
Nu's `source` directive also consult `VirtualPath`, or does it call
`std::fs` directly? If the former, this also fixes our hub-bundling
problem (`scripts/bundle-cf-handler.nu` becomes optional). If the
latter, vfs.rs only helps `use`, not `source`.

## Per-demo scope (after Workspace layering + mirror)

### templates (~half day)
- Mirror `append_command.rs` -> `commands/append.rs`
- Mirror `cat_command.rs` -> `commands/cat.rs`
- Build `store.rs` substrate
- Wire into `cf/mod.rs::engine()` after the existing shadows

### quotes (~1 day on top)
- Mirror `last_command.rs` -> `commands/last.rs`
- Mirror `last_stream_command.rs` -> `commands/last_stream.rs`
- `--follow` uses Workspace `on_change` listener; broker pattern so
  one persistent DO-level listener fans out to N SSE connections

### 2048 gameplay (~1 day on top)
- `.bus` is xs-like but in-memory. On CF: write to
  `/.xs/bus/<topic>/<id>.json` with TTL::Ephemeral; delete after emit
- Reuses the follow plumbing from quotes

### stor (~2 days, separate track)
- Doesn't fit the file model. Direct `worker::SqlStorage` passthrough.
- Plan at [`../nu_command/stor/README.md`](../nu_command/stor/README.md).

## Smallest entry point

`src/cf/nu/xs/commands/append.rs` (mirroring `append_command.rs`) +
`store.rs` (Workspace-backed). Half a day, validates the
Workspace-as-substrate theory, unblocks templates.

## Working rules (will move to CLAUDE.md when implementing)

1. Path-for-path mirror with xs. Every command's module doc has an
   `Upstream:` line citing `xs/src/nu/commands/<file>.rs:LINE
   <CommandName>`.
2. Frame shape mirrors xs's `Frame` exactly. Pin the xs revision in
   a comment.
3. Same `Ok(None)`-on-not-found discipline as Workspace.
4. `Store` method names match xs's `Store` so command file diffs are
   minimal.
5. After every edit, both targets compile (`cargo check` + the wasm
   target).
