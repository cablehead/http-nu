# `src/cf/shell/` -- working rules for Claude

This directory holds the **wasm-only** half of the `@cloudflare/shell`
Rust port: today, the `Workspace` impl (DO SQLite + R2) and its SQL
schema.

The **backend-agnostic** half (the `FileSystem` trait, shared types,
`InMemoryFs`, `FsError`, `path_utils`, conformance tests) lives at top
level in [`src/shell/`](../../shell/) so it's reachable from desktop.
Read [`src/shell/CLAUDE.md`](../../shell/CLAUDE.md) before touching
anything here -- the trait + mock-divergence rules apply to this
directory too.

## 1. File layout mirrors upstream, path-for-path

Filenames here match files under `.src/agents/packages/shell/src/`.
When you add a new wasm-only file:

- Find the upstream sibling first.
  `.src/agents/packages/shell/src/<x>.ts` -> `src/cf/shell/<x>.rs`
- TS kebab-case -> Rust snake_case (e.g. `path-utils.ts` ->
  `path_utils.rs`).
- If the upstream file is desktop-compatible (no `worker::*` deps),
  it goes in [`src/shell/`](../../shell/), NOT here.
- If there is no upstream sibling (e.g. `schema.rs` -- extracted from
  inline DDL in `filesystem.ts`), document why at the top of the file
  AND note the exception in `PORT_STATUS.md`'s file-level table.

Never reorganise into a "more idiomatic" Rust shape. The cost of a
divergent layout is paid forever on every merge from upstream.

## 2. Provenance: every public item gets an `Upstream:` line

Every `pub fn`, `pub struct`, `pub enum`, `pub const` in this
directory MUST start its doc comment with one of:

```rust
/// Upstream: filesystem.ts:526 `readFile()`.
pub async fn read_file(...) { ... }

/// Port-only: <reason why no upstream equivalent>.
pub async fn realpath(...) { ... }
```

The `filename:line methodName()` form is non-negotiable. It is what
makes a reviewer's job mechanical: open the TS file at that line,
diff the bodies. When upstream restructures and our line ref drifts,
fix it; never delete it.

When porting a *new* method or type:

1. Find the upstream line. `grep -nE "^  (async |public )?<name>\(" .src/agents/packages/shell/src/<file>.ts`.
2. Write the `/// Upstream: <file>.ts:NN <camelCaseName>` line.
3. Note any deviation immediately after the Upstream line, in the
   same comment block.

## 3. Schema is the interop contract

Anything that touches the DB or R2 layout (table name, column types,
CHECK constraints, R2 key shape) is byte-compatible with
`@cloudflare/shell@<X>`. The canonical version is pinned in
`PORT_STATUS.md`'s "Schema compatibility" table; the canonical DDL lives in
`schema.rs::create_table_sql`.

If you change anything in that table, you must:

1. Verify upstream agrees (read the SQL strings in
   `.src/agents/packages/shell/src/filesystem.ts`'s init paths).
2. Update `PORT_STATUS.md`'s schema table in the same edit.
3. Round-trip test: write from one side, read from the other.

A divergent schema breaks the entire reason this port exists. Treat
this rule as a brick wall.

## 4. POSIX error-prefix convention

Every `worker::Error::RustError` in this directory starts with a
POSIX errno-style prefix:

`ENOENT:` (no such file) | `EISDIR:` (is a directory) |
`ENOTDIR:` (not a directory) | `EEXIST:` (already exists) |
`ENOTEMPTY:` (directory not empty) | `ENAMETOOLONG:` (path too long) |
`ELOOP:` (symlink loop) | `EILSEQ:` (invalid byte sequence) |
`EIO:` (I/O error) | `ENOSPC:` (no space).

Match upstream's exact phrasing where it has a precedent (e.g.
`EISDIR: cannot write to root directory`, `ENOENT: no such file or
directory: <path>`). The prefix lets callers pattern-match on error
kind without parsing English.

## 5. PORT_STATUS.md is the running ledger

When you port a method, change a signature, or fix a parity gap:

- Update the relevant row in `PORT_STATUS.md`'s method-level table (move it
  out of "not ported", change its `Rust line` column, add a deviation
  note if the signature differs from upstream).
- If you fix a behavioral gap from the "Behavioral parity" section,
  move it into that section as a "done" item (or remove the bullet if
  it now matches upstream exactly).
- When you start porting a new module, add a row to the file-level
  table with `Rust line: in progress` so it shows up in the ledger
  before the file is complete.

The README.md, CLAUDE.md (this file), and module doc comments are
durable; `PORT_STATUS.md` is the running state. Keep it fresh.

## 6. After every edit: both targets must compile

```
cargo check                                                              # desktop
CF_HANDLER_PATH=../../examples/cf-workspace-browser/serve.nu \
  cargo check --target wasm32-unknown-unknown --features cloudflare --no-default-features
```

Both. Never one. The whole point of `#[cfg(feature = "desktop")]` is
that desktop never breaks; the whole point of the wasm gate is that
this port stays Workers-shippable.

## 7. When in doubt, read upstream

`.src/agents/packages/shell/` is a local clone (gitignored). Grep it
before guessing. If you can't tell whether a deviation is intentional,
the answer is in the TS file at the line you cited in your Upstream
ref.

The package version we track is `@cloudflare/shell@0.3.6` per
`.src/agents/packages/shell/package.json`. When the local clone gets
refreshed and the upstream version bumps, also bump the version
reference in `PORT_STATUS.md`'s intro and re-audit any line refs that may
have shifted.
