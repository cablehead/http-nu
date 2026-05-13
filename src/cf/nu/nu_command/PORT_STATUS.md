# Nu shadow commands -- port status

Running ledger of `nu-command` shadows registered for the CF target.
Companion to `README.md` (durable overview) and `CLAUDE.md` (contributor
checklist). When you add, remove, or change a shadow, update the tables
here in the same edit.

- **Upstream package:** `nu-command` from
  [`nushell/nushell`](https://github.com/nushell/nushell)
  (local clone: `.src/nushell/crates/nu-command/`, gitignored).
- **Dependency version (Cargo.toml):** `nu-command = "0.112.1"`
- **Local clone version (`.src/nushell/Cargo.toml`):** `0.112.3`
- **Registration site:** `src/cf/mod.rs::engine()` -- shadows are added
  via `engine.add_commands(...)` AFTER `add_custom_commands`, so they
  take priority over stock declarations.

## Demand map (the scoping discipline)

**A shadow exists in this directory because a specific example or real
script needs it.** Not "this command exists upstream" or "this might
be useful." This section is the canonical evidence of what's
demanded; when adding or removing a shadow, update it in the same
edit.

**Why this matters.** Nushell is large: ~25 nu-* crates, hundreds of
commands, an extensive test suite. The reflex when looking at the
upstream surface is "we need to mirror all of it." We don't. The
actual demand from http-nu's examples is small (~14 commands). This
map locks the scope so future-us doesn't drift into "shadow
everything" mode.

**The leverage** is documented in
[`README.md`](README.md)'s "Structural picture: BASE + LEAF" section:
nushell splits into three layers; we only own the bottom one
(OS-touching commands), and Layers 1 and 2 inherit our shadows
automatically through Nu's name-lookup at parse time. That's the
strategy that keeps this map short. Don't forget it.

### Shadowed (11) -- each line cites the example that demands it

Sourced by grep across `examples/*/serve.nu`. Numbers are
appearance counts (rough -- includes some false positives for short
names like `ls`).

| Shadow         | Used by examples                                   | Apps      |
|----------------|----------------------------------------------------|-----------|
| `ls`           | `cf-workspace-browser`, others                     | many      |
| `open`         | `cf-workspace-browser`, others                     | 5         |
| `save`         | `cf-workspace-browser`                             | 2         |
| `mkdir`        | `cf-workspace-browser`                             | 3         |
| `rm`           | `cf-workspace-browser`, others                     | many      |
| `cp`           | `cf-workspace-browser`                             | 3         |
| `mv`           | `cf-workspace-browser`                             | 3         |
| `glob`         | `cf-workspace-browser`                             | (audit)   |
| `path self`    | `mermaid-editor` (partially)                       | 4         |
| `path exists`  | common pattern                                     | 2         |
| `sleep`        | `2048`, `basic`                                    | 2         |

### Working without a shadow (via `nu-command/js` feature)

These come along for free with the `cloudflare` feature -- no shadow,
no maintenance. Verify against `nu-command/Cargo.toml` `[features]`
before assuming continued coverage:

- `path join`, `path dirname`, `path basename` -- pure path utils, no
  OS lookup.
- `date now`, `format date`, `date format`, `date humanize`,
  `date to-record`, `date to-timezone` -- `js` swaps `chrono`'s time
  source.
- `random integer`, `random float`, `random bool`, `random chars`,
  `random uuid`, `random binary`, `random dice` -- `js` swaps the
  RNG seed source.
- All pure-data ops (`from json`, `to json`, `where`, `sort-by`,
  `group-by`, `select`, `update`, `each`, `reduce`, string/math/list
  /record commands) -- no OS dependency.
- `generate` -- verified on CF via
  [`examples/generate-test/serve.nu`](../../../../examples/generate-test/serve.nu),
  returns `1,1,2,3,5,8,13,21,34,55` end-to-end. The `generators/`
  module isn't feature-gated upstream; works through stock with no
  shadow needed.

### Used by examples but NOT shadowed -- shadow targets

| Command | Used by | Upstream path | Destination | Status |
|---------|---------|---------------|-------------|--------|
| `stor`  | `stor`  | `nu-command/src/stor/` (+ `database/query_db.rs` + the `sqlite-in-memory` custom value) | `nu_command/stor/<files>` + new `nu_command/database/query_db.rs` | **NOT STARTED.** Backend choice open: DO SQLite (sync, what Workspace uses) vs D1 (async, blocked by Nu-sync-eval). See [`stor/README.md`](stor/README.md). No `.rs` files yet — a stub would be a footgun. |

### Used by examples but BLOCKED by async Nu eval (not shadow material)

`fetch` (`http get` / `http post`) belongs here, not in shadow
targets. Same structural blocker as `sleep`:

- Workers `fetch()` is async-only on wasm.
- Nu's `Command::run(&self, ...)` is sync.
- You can't `.await` inside a sync fn, and wasm has no `block_on`.

| Command | Used by examples | Why blocked                                                                |
|---------|------------------|----------------------------------------------------------------------------|
| `fetch` / `http get` / `http post` | `2048` | Stock uses `reqwest` (won't compile to wasm); Workers `fetch()` is async only. Same blocker as `sleep`. Unblocked by the async Nu eval refactor (months upstream) or a side-channel `.fetch` custom command on the `RESPONSE_TX` pattern. |
| `sleep` (real, not no-op) | `basic`, `2048` | Already shadowed as a no-op; real timing requires async yield. |

### Used by examples but needs a DIFFERENT BACKEND (not a shadow)

These can't be solved by a shadow in this directory. They need work in
adjacent repos:

| Command(s)              | Used by examples | What's needed                                       |
|-------------------------|------------------|-----------------------------------------------------|
| `.bus`, `.cat`, `.append` | `2048`, others | xs CF backend (`xs` repo). Maps `fjall` -> DO SQLite, `cacache` -> R2. |

### What's intentionally NEVER shadowed

| Upstream surface | Why                                                                |
|------------------|--------------------------------------------------------------------|
| `nu-cli`         | REPL only; gated `optional = true` and only loaded on desktop.     |
| `nu-plugin*`     | Workers can't host external processes; gated `optional = true`.    |
| `nu-table`       | TTY rendering; not relevant in a server.                           |
| `nu-explore`     | Interactive UI.                                                    |
| Everything in `nu-cmd-extra` not exercised by examples | Not currently demanded.|

If an example starts using something from these, that's the trigger
to revisit.

## Shadow table

Every row is a Nu builtin that we replace at engine init. The "Why
shadowed" column maps to one of the three reasons in `CLAUDE.md` rule
3: **vfs** (routes filesystem through `Vfs`), **wasm** (stock parse-
errors or panics on `wasm32-unknown-unknown`), **cpu** (stock would burn
the Workers CPU budget).

| Nu command       | Upstream (`nu-command/src/`)  | Shadow (`src/cf/nu/nu_command/`) | Registered at (`cf/mod.rs`) | Why shadowed | Notes                                                                 |
|------------------|-------------------------------|------------------------------|------------------------------|--------------|-----------------------------------------------------------------------|
| `ls`             | `filesystem/ls.rs`            | `filesystem/ls.rs`           | L97                          | vfs          | Returns a table of `name`/`type`/`size`; reads via `Vfs::read_dir_with_stat`. |
| `open`           | `filesystem/open.rs`          | `filesystem/open.rs`         | L98                          | vfs          | Reads via `Vfs::read_file_bytes`; mime sniffing inherited from Vfs row's `mime_type`. |
| `save`           | `filesystem/save.rs`          | `filesystem/save.rs`         | L99                          | vfs          | Writes via `Vfs::write_file_bytes`; pipeline string -> utf8 bytes; binary pipeline -> raw bytes. |
| `path exists`    | `path/exists.rs`              | `path/exists.rs`             | L100                         | vfs          | Pipeline or arg path; calls `Vfs::exists`. Stock would hit `Path::is_absolute()` (wasm-broken). |
| `mkdir`          | `filesystem/mkdir.rs`         | `filesystem/mkdir.rs`        | L101                         | vfs          | `recursive` flag mirrors stock; calls `Vfs::mkdir`.                  |
| `rm`             | `filesystem/rm.rs`            | `filesystem/rm.rs`           | L102                         | vfs          | `recursive`/`force` mirror stock; calls `Vfs::rm`.                   |
| `cp`             | `filesystem/cp.rs`            | `filesystem/cp.rs`           | L103                         | vfs          | `recursive` mirrors stock; preserves source `mime_type` (matches Vfs).|
| `mv`             | `filesystem/mv.rs`            | `filesystem/mv.rs`           | L104                         | vfs          | Calls `Vfs::mv`; rename semantics match stock when target is a dir.   |
| `glob`           | `filesystem/glob.rs`          | `filesystem/glob.rs`         | L105                         | vfs          | Calls `Vfs::glob`; pattern syntax matches upstream `glob` crate.      |
| `path self`      | `path/self_.rs`               | `path/self_.rs`              | L108                         | wasm         | Stock calls `engine_state.cwd()` -> `Path::is_absolute()` -> false on wasm -> parse-time error. Shadow returns a workspace-rooted path; `is_const = true` so `const x = path self` still works. |
| `sleep`          | `platform/sleep.rs`           | `platform/sleep.rs`          | L113                         | cpu          | NO-OP with one logged warning per call. Scripts that loop on `sleep` will spin until the Workers CPU limit trips -- everything else parses and runs normally. |

## Divergences from stock (index)

The full flag-by-flag table for each shadow lives in **the shadow's
own module doc** (top of the `.rs` file). Co-located with the code so
the table can't drift when the `signature()` changes. This section is
just the index + audit status:

| Shadow         | File                        | AUDIT pending? |
|----------------|-----------------------------|----------------|
| `ls`           | `filesystem/ls.rs`          | no             |
| `open`         | `filesystem/open.rs`        | **yes (2)**    |
| `save`         | `filesystem/save.rs`        | **yes (3)**    |
| `mkdir`        | `filesystem/mkdir.rs`       | no             |
| `rm`           | `filesystem/rm.rs`          | **yes (1)**    |
| `cp`           | `filesystem/cp.rs`          | **yes (1)**    |
| `mv`           | `filesystem/mv.rs`          | **yes (1)**    |
| `glob`         | `filesystem/glob.rs`        | **yes (2)**    |
| `path exists`  | `path/exists.rs`            | **yes (1)**    |
| `path self`    | `path/self_.rs`             | no             |
| `sleep`        | `platform/sleep.rs`         | no             |

AUDIT pending = the shadow's module doc has rows marked `unknown --
AUDIT`. Walk the shadow's `signature()` against the corresponding
upstream file and either confirm the flag works (change to `yes`),
implement it, or change to `no` with a reason. Tracked as a gap
below.

When `nu-command` bumps, re-audit each shadow's table against the
new `.src/nushell/crates/nu-command/src/<cat>/<name>.rs`.

## Shared infrastructure

| File                              | Purpose                                                              |
|-----------------------------------|----------------------------------------------------------------------|
| `mod.rs`                          | Module entry; re-exports all shadow types.                           |
| `shared.rs`                       | `normalise_input`, `vfs_err`, `no_vfs`, `require_vfs`. Used by every Vfs-backed shadow. |
| `filesystem/mod.rs`               | Re-exports filesystem shadows.                                       |
| `path/mod.rs`                     | Re-exports path shadows.                                             |
| `platform/mod.rs`                 | Re-exports platform shadows.                                         |

## Other gaps (beyond the demand-map shadow targets)

The demand map above lists the three example-driven shadow targets
(`generate`, `stor`, `fetch`). The items below are quality / coverage
work that doesn't add a new shadow but matters:

1. **Resolve the "unknown -- AUDIT" rows** in the Divergences from
   stock tables. Each one means a flag is in stock's signature but a
   reader hasn't yet compared it against our shadow. Reading each
   shadow's `signature()` and marking "yes" / "no with reason" /
   implementing it -- maybe an hour total; reduces silent-drift risk.
2. **Nu-script conformance suite.** Same shape as
   `src/shell/conformance.rs`: `.nu` scripts that run against BOTH
   stock (desktop) and shadow (CF) and diff stdout. The divergence
   tables are documentation; this is enforcement.
   - `tests/nu_conformance/<command>/<case>.nu` (script taking a
     `base` path arg) + `.expected.json`.
   - `mise run nu:conformance:desktop` -- runs each via `cargo run`
     in a tmpdir.
   - `mise run nu:conformance:cf` -- POSTs each to a new debug route
     in a fresh `__conformance_nu` workspace.
   - `mise run nu:conformance` -- runs both; fails if any divergence.
   Maybe a day to wire up; another day or two to seed cases from
   upstream nu-command tests.
3. **Upgrade `sleep` to a real async yield** once Nu grows an async
   eval path. Today the no-op is the only honest option.

## Stale-shadow audit

Re-run this check when bumping `nu-command`:

For each row in the shadow table where "Why shadowed" is **wasm** or
**cpu** (not **vfs**), grep the new upstream version for the failing
call (e.g. `Path::is_absolute`, `std::thread::sleep`). If upstream
fixed it under `js` feature, move the row to "no longer needed"
section below and remove the shadow registration in `cf::mod.rs`.

**vfs** shadows are durable -- as long as Workers has no disk, those
stay regardless of upstream changes.

## No longer needed

(Empty.) When a shadow becomes redundant because a `nu-command`
version made stock work on wasm, move the row here with the version
that fixed it and a one-line note.
