# `src/cf/nu/nu_command/` -- working rules for Claude

This directory holds Nu shadow commands: `Command` impls that *replace*
stock `nu-command` builtins at engine init time, registered after
`add_custom_commands` so they win the name lookup. The whole point is
that **a reviewer who knows stock Nu can read our shadow side-by-side
with the upstream `nu-command` file it replaces**. Every rule below
exists to keep that property.

## ⚠️ The shadow-divergence trap

A shadow is structurally identical to a *mock* of stock `nu-command`:
same name, same intent, separate implementation. So it has the same
failure mode the FileSystem port did (see
[`crates/cloudflare-shell/CLAUDE.md`](../../../../crates/cloudflare-shell/CLAUDE.md)) -- silent drift:

1. Stock `ls` on desktop supports `--all`, `--long`, sort flags, etc.
2. Our shadow `ls` on CF supports only `path`.
3. A Nu script that runs on desktop with `ls --long` produces a rich
   table; on CF the same script fails to parse OR silently drops the
   flag.
4. Tests against the CF shadow alone pass. Production behaviour
   diverges from desktop. User sees confusing differences only after
   shipping.

**A shadow that drifts from stock and isn't tracked is worse than no
shadow** -- it ships false confidence. The defences are documentation
and (eventually) tests.

### Defence 1: PORT_STATUS.md tracks every divergence

`PORT_STATUS.md`'s **"Divergences from stock"** subsection lists, for
each shadow, every flag/feature stock supports vs what we implement.
When you add or modify a shadow:

1. Open the corresponding `.src/nushell/crates/nu-command/src/<cat>/<name>.rs`.
2. Read its `signature()`. Note every `.switch`, `.named`,
   `.optional`, `.required`, `.rest`, `.input_output_types`.
3. Open the matching `PORT_STATUS.md` row. Confirm the divergence
   notes are complete. If you've added a new flag, document it. If
   you've left one unimplemented, document THAT too (silent gaps are
   the failure mode).
4. If the deviation isn't representable in a one-line note, expand
   the module doc on the shadow file (rule 2 below).

### Defence 2: Module doc on each shadow lists deviations

Every shadow file's module doc carries a **`Divergences from stock:`**
block immediately after the `Mirrors` line. Format:

```rust
//! `ls` shadow. Mirrors `nu-command/src/filesystem/ls.rs`.
//!
//! Divergences from stock:
//! - No `--all` / `-a`: workspace has no hidden-file convention.
//! - No `--long` / `-l`: skipped for v1; would need group, perm,
//!   timestamp columns.
//! - Sort is name-ascending only; stock supports `--full-paths`
//!   ordering options.
//! - Returns Vfs `entry.kind` instead of stock's file-type-with-link
//!   tagging.
```

If a shadow has zero divergences, say so explicitly: `Divergences
from stock: none -- behaviour parity verified against
nu-command 0.112.1.` Lying is worse than admitting a gap.

### Defence 3 (future): Nu-script conformance suite

Same shape as `cloudflare-shell::conformance`: a directory of `.nu` test
scripts that run against BOTH desktop (stock) and CF (shadow) and
diff outputs. Not built today; tracked under "Gaps" in
`PORT_STATUS.md`. The per-demo parity check in CLOUDFLARE.md is the
smoke-test approximation we have until then.

### What the discipline buys

Without it: shadow drifts silently, demos work, subtle divergences
ship. With it: every shadow's deviation is in writing; reviewers can
audit; future Nu-script conformance tests can be written from the
documented gaps.

## 1. When to add a shadow (the gate)

A shadow goes in this directory only if **both** are true:

1. **Demand:** a specific example or real script needs the command.
   Cite it in the module doc:
   ```rust
   //! Used by: `examples/2048/`, `examples/basic/`.
   ```
   If nothing ships uses it, don't add it. Upstream nu-command will
   keep adding commands forever; that's not our problem until
   something we ship calls them.

2. **Structural reason** stock can't handle it on wasm. Exactly one of:
   - **vfs** -- command touches `std::fs`; route through
     `crate::vfs::Vfs` (Workers has no disk).
   - **wasm** -- stock parse-errors or panics on wasm32 (e.g. `path
     self` calls `Path::is_absolute()`, always false on wasm). Cite
     the failing call in the module doc.
   - **cpu** -- stock would burn the Workers CPU budget (e.g.
     `sleep`, which has no async yield in sync Nu commands).
   If none of the three applies, check `nu-command/Cargo.toml`
   features -- the `js` feature already wasm-fixes many commands
   (we enable it via our `cloudflare` feature). If `js` covers it,
   don't shadow.

Then in the same edit: write the shadow, register it in
`src/cf/mod.rs::engine()`, and update the **Demand map** in
[`PORT_STATUS.md`](PORT_STATUS.md). When a shadow is still a
*target* (not yet implemented), its demand-map row must include the
upstream path *and* the destination path here -- so the next
contributor doesn't have to re-derive the file layout. See the
existing rows for `generate` / `stor` / `fetch` for the format.

### Why this gate exists -- the BASE + LEAF strategy

Nushell is large -- ~25 nu-* crates, hundreds of commands. The
reflex is "we need to mirror all of it." We don't, because the
stack splits into three layers and **we only own the bottom one**:

- **BASE** (nu-protocol / nu-engine / nu-parser / nu-cmd-lang /
  nu-cmd-extra / nu-std / nu-utils) -- compiles to wasm; just
  works once enabled.
- **FREE under `nu-command/js`** -- pure-data ops, path utils, date,
  random. No code from us.
- **LEAF** (this directory) -- the OS-touching commands. Bridge
  `nu-command` -> `Vfs` once; everything Nu-side (stdlib, user
  scripts, pipelines) inherits it through Nu's name lookup at parse
  time.

[`README.md`](README.md)'s "Structural picture" section spells this
out in full. **This is the strategy that makes the demand map short
-- don't forget it.**

Same principle holds for `crates/cloudflare-shell-workspace/` (port
only what http-nu calls; see [`crates/cloudflare-shell/CLAUDE.md`](../../../../crates/cloudflare-shell/CLAUDE.md)).

## 2. File layout mirrors `nu-command/src/`, path-for-path

Filenames here match files under
`.src/nushell/crates/nu-command/src/<category>/<name>.rs`. When you
add a new shadow:

- Find the upstream sibling first.
  `nu-command/src/filesystem/ls.rs`  -> `src/cf/nu/nu_command/filesystem/ls.rs`
  `nu-command/src/path/exists.rs`    -> `src/cf/nu/nu_command/path/exists.rs`
  `nu-command/src/platform/sleep.rs` -> `src/cf/nu/nu_command/platform/sleep.rs`
- Reserved-name rule: `nu-command/src/path/self_.rs` keeps the `_`
  suffix (TS-style mirror isn't relevant; Rust filenames can't be
  `self.rs`).
- If there is no upstream sibling, document why at the top of the file
  AND note the exception in `PORT_STATUS.md`'s shadow table.

Never reorganise into a "more idiomatic" Rust shape (one big
`filesystem.rs` for all FS shadows, etc.). The cost of a divergent
layout is paid forever on every nu-command bump.

## 3. Provenance: every shadow's module doc cites upstream

Every file in this directory MUST open with a module-level comment of
the form:

```rust
//! `ls` shadow. Mirrors `nu-command/src/filesystem/ls.rs`.
```

When the shadow deviates from stock (parse-time bug, semantic change,
no-op stub), the deviation goes in the same comment block, immediately
after the `Mirrors` line. Example from `platform/sleep.rs`:

```rust
//! `sleep` shadow. Mirrors `nu-command/src/platform/sleep.rs`.
//!
//! CF target: NO-OP. Workers' async event loop doesn't expose a sync
//! sleep, and Nu commands are sync; ...
```

When upstream restructures and the line ref drifts, fix it; never
delete it.

## 4. Filesystem shadows route through `Vfs` only -- no `std::fs`

In this directory, `std::fs::*` is banned. All filesystem reads/writes
go through `crate::cf::nu::nu_command::shared::require_vfs` (which calls
`crate::vfs::with_vfs`). The Vfs is installed per request in
`cf::mod.rs::fetch`; commands that need it MUST handle the
"no Vfs installed" case via `require_vfs` (returns a `ShellError` with
the standard "Workspace not loaded" message).

Path normalisation is also centralised: use
`shared::normalise_input` for any user-supplied path so `"."`, `"./"`,
`""` and bare names map consistently to absolute workspace paths.

## 5. Registration is part of the port

A shadow file in this directory is dead code until it's added to the
`add_commands` list in `src/cf/mod.rs::engine()`. When you add a new
shadow:

1. Implement it under `src/cf/nu/nu_command/<cat>/<name>.rs`.
2. Re-export from the category `mod.rs` and from `commands/mod.rs`.
3. Add `Box::new(commands::<Name>)` to the `add_commands` vec in
   `cf::mod.rs::engine()` -- AFTER `add_custom_commands`, so the
   shadow wins the name lookup.
4. Update `PORT_STATUS.md`'s shadow table in the same edit.

Forgetting step 3 is the single most common failure mode -- the file
compiles, the test passes locally for "is `VfsXxx` a `Command`?", and
the deployed worker still routes to the stock command.

## 6. Error convention

Shadows return `ShellError`, not `worker::Error` -- they're inside the
Nu engine, not the worker handler. Use the helpers in `shared.rs`:

- `shared::vfs_err(span, msg, error)` -> `ShellError::GenericError`.
- `shared::no_vfs(span)` -> the canonical "Workspace not loaded" error.
- `shared::require_vfs(span, |v| ...)` -> wraps Vfs access with the
  no-vfs error handling.

The Vfs itself emits POSIX-prefixed error strings (`ENOENT:`, `EISDIR:`,
etc., per `crates/cloudflare-shell-workspace/CLAUDE.md`); when surfacing those through a
shadow, pass the Vfs error string into `vfs_err`'s `error` field
unchanged so callers downstream of `do $closure` can still
pattern-match on the prefix.

## 7. Three doc levels (and when each applies)

Docs live at the level that's hardest to forget when changing the code:

| Level                                 | What lives there                                              | When                                                                |
|---------------------------------------|---------------------------------------------------------------|---------------------------------------------------------------------|
| Top of this directory (`nu_command/`) | `README.md` (orientation), `CLAUDE.md` (these rules), `PORT_STATUS.md` (demand map + shadow table + divergence INDEX + gaps) | Always.                                                             |
| **Category folder** (`<cat>/`)        | `README.md` + `CLAUDE.md` ONLY when the category has multiple subcommand files sharing a cross-cutting concern (e.g. `stor/` shares the DO SQLite client; `network/http/` shares the Workers `fetch()` wiring). Holds the shared rationale + any shared helpers' contract. | When a category contains >1 file *and* there's something shared to say. Solo-shadow categories (`platform/sleep.rs`) skip this level. |
| Shadow file (`<cat>/<name>.rs`)       | Module doc with: `Mirrors <upstream path>`, `Used by:` citation, `Divergences from stock:` flag-by-flag table. | Every shadow.                                                       |

The full divergence table is in the **module doc**, not
`PORT_STATUS.md`. `PORT_STATUS.md`'s divergence section is just the
index + audit status. Co-location is what stops the table from
drifting when `signature()` changes.

## 8. PORT_STATUS.md is the running ledger

When you add, remove, or change a shadow:

- Update the shadow table row in `PORT_STATUS.md` (file path,
  registration site line, deviation note).
- If you're removing a shadow because stock now works (e.g. a
  `nu-command/js` feature picked it up), move the row to the "no
  longer needed" section with the upstream version that made the
  shadow redundant.

The README is durable; this CLAUDE.md is durable; `PORT_STATUS.md` is
the running state. Keep it fresh.

## 9. After every edit: both targets must compile

```
cargo check                                                              # desktop
CF_HANDLER_PATH=../../examples/cf-workspace-browser/serve.nu \
  cargo check --target wasm32-unknown-unknown --features cloudflare --no-default-features
```

Both. Never one. Desktop builds this code under `#[cfg(feature =
"desktop")]` gates in `cf::mod.rs`; if a shadow accidentally pulls
something that desktop can't link, desktop breaks silently until CI.

## 10. When in doubt, read upstream

`.src/nushell/` is a local clone (gitignored). Grep the
`crates/nu-command/src/<category>/` subdir before guessing. If the
shadow's behaviour diverges from stock and you can't tell whether
it's intentional, the answer is in the `.rs` file at the path you
cited in your `Mirrors` line.

The `nu-command` version we depend on is pinned in the workspace
Cargo.toml (`nu-command = "0.112.1"` at time of writing); the local
clone in `.src/nushell/` may be slightly newer (e.g. `0.112.3`). When
they skew enough to matter, refresh the clone and re-audit cited
paths.
