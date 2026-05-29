# `nu_command/stor/` -- port plan (NOT YET STARTED)

This folder is **reserved** for the `stor *` family shadow. It is
empty by design. **Nothing here is implemented or registered.**

This README is the design plan for the future port. When someone
picks up the work, the steps below are the spec.

## Why this folder exists at all

We follow a strict rule (see `../CLAUDE.md` §7): a category-level
`README` lives at the directory that holds multiple subcommand files
sharing a cross-cutting concern. `stor *` will be exactly that shape
(5+ subcommand files sharing a SQL backend). The folder + this
README is the **design reservation**, not progress.

If you see no `.rs` files here, that is correct. Nothing has been
ported yet. Anyone who adds files here must take responsibility for
making them actually work AND registering them in
`src/cf/mod.rs::engine()` in the same edit.

## Why a shadow is needed

Stock `stor` (in `nu-command/src/stor/`) is gated behind
`nu-command/sqlite`, which pulls `rusqlite` -- C bindings, won't
compile to wasm. Our `cloudflare` Cargo feature leaves `sqlite`
off, so the whole `nu_command::stor` family is absent from our
wasm build. The `stor` example fails at parse time on CF.

## Backend choice -- this is an OPEN QUESTION, not a default

CF has **two** SQLite-ish backends. The choice has real consequences;
pick deliberately, document the call here before writing code.

| Backend                      | API           | Scope                     | Lifecycle                                | Matches stock semantics?                                            |
|------------------------------|---------------|---------------------------|------------------------------------------|---------------------------------------------------------------------|
| **DO SQLite** (`worker::SqlStorage`) | **sync** `exec()` | per-DO (= per-user with our routing) | persists across requests until DO eviction | partial: sync API matches; persistence diverges from stock's "in-memory ephemeral" |
| **D1** (`worker::D1Database`)        | **async**         | global / cross-DO          | durable, Cloudflare-managed              | partial: lifecycle closer to a real database; sync mismatch means it can't be called from Nu's sync `Command::run` |

### What each implies

- **DO SQLite path:** doable today, sync API plugs into Nu commands
  directly, BUT stor tables share DO storage budget with our
  `cf_workspace_*` tables -- discipline required (table-name prefix
  `stor_*`, see below). Data persists between requests for the same
  user, which scripts expecting stock's "wiped on process exit"
  semantics will notice.
- **D1 path:** semantically closer to a real database, but D1 is
  async-only on the Workers side. A sync Nu command can't `.await`,
  same blocker as `fetch` / `http get`. Either wait for async Nu
  eval (months upstream), or build a side-channel pre-fetch like the
  `.static` / `RESPONSE_TX` pattern -- which changes the user-facing
  API shape away from stock `stor`.

### Recommended default

**DO SQLite** for the first port. Rationale:

1. Sync API is the only one that fits Nu's `Command::run` today.
2. Per-DO scoping aligns with our existing per-user model (handler
   reload, Workspace) -- no new infra.
3. Lifecycle divergence from stock is documentable; sync-vs-async
   isn't recoverable without months of upstream work.

D1 is then the future variant when (a) async Nu eval lands, OR (b)
we genuinely need cross-user `stor` (we don't, today). When that
day comes, the impl lifts cleanly into a sibling backend behind the
same shadow surface.

**This decision is the load-bearing one for the port** -- it dictates
table-namespacing rules (point 2 below), sync vs async signatures,
and what "wiped between runs" means to a user. If you implement this
shadow with a different backend choice than DO SQLite, update this
README in the same edit and explain why.

## Demand: `examples/stor.nu` -- bigger than it looks

Reading the actual example surfaces a dependency I almost missed:
the example's hot path is

```nu
stor create -t visits -c {path: str ts: str method: str} | ignore
{path: $req.path, ts: ..., method: ...} | stor insert -t visits | ignore
stor open | query db "select path, count(*) as n from visits group by path order by n desc"
stor open | query db "select * from visits order by ts desc limit 10"
```

`stor open` returns a Nu **custom value** of type `sqlite-in-memory`
(upstream type signature: `Type::Custom("sqlite-in-memory")`).
`query db` consumes that custom value from its pipeline input and
runs arbitrary SQL against it.

That means a faithful port needs:

1. A custom-value type that wraps our DO `SqlStorage` handle
   (or a namespace pointer to it). Implementing the Nu `CustomValue`
   trait (~10 required methods: `clone_value`, `type_name`,
   `to_base_value`, `as_any`, `notify_plugin_on_drop`, plus
   serialize/deserialize via `Cow`, etc.).
2. **Two** command categories, not one:
   - `nu_command/stor/` -- this folder
   - `nu_command/database/` -- a new sibling for `query db` (and
     probably the conversion helpers it depends on)
3. The custom-value type lives somewhere accessible to both
   categories; likely `nu_command/database/value.rs` or hoisted
   into a shared module.

So minimum viable subcommand set for the example to RUN is:

- `stor` (the dispatcher, returns help)
- `stor create` (CREATE TABLE)
- `stor insert` (INSERT row)
- `stor open` (returns the custom value)
- `query db` (consumes the custom value, runs arbitrary SQL,
  returns a Nu table)
- `stor delete` -- NOT used by the example; defer

Other subcommands (`update`, `import`, `export`, `reset`) wait for
later demand.

### Why this matters before writing any code

Porting `stor *` alone (without `query db`) ships a parse-time fix
but leaves the example broken at runtime (`stor open` returns a value
that nothing on CF can consume). That's a footgun: parse-green but
runtime-red looks like progress and isn't. The honest scope is the
full chain above OR don't ship.

## Implementation spec

The closest in-repo pattern is
[`cloudflare-shell-workspace::filesystem::Workspace::write_inner`](../../../../../crates/cloudflare-shell-workspace/src/filesystem.rs)
-- same sync `SqlStorage::exec()` call shape, same Value <-> SQL row
concerns. Lift from there.

### Cross-cutting concerns (the reason this is a folder, not flat files)

1. **All subcommands share one `worker::SqlStorage` handle.** Get it
   from the per-request Vfs/Workspace context the same way
   filesystem shadows reach `require_vfs`. Plan: a `shared::stor_sql()`
   helper in `nu_command::stor` (or hoisted into the parent
   `nu_command::shared`) that returns the per-request SQL handle.
2. **Table-name prefix.** All `stor`-managed tables go to
   `stor_<name>` to avoid colliding with `cf_workspace_*` tables in
   the same DO storage.
3. **Type mapping.** Nu `Value` <-> SQL `TEXT` / `INTEGER` / `REAL`
   / `BLOB`. Mirror what
   `nu-command::database::convert_sqlite_row_to_nu_value` does
   upstream, but smaller.

### Step-by-step

When porting:

1. Read `.src/nushell/crates/nu-command/src/stor/<file>.rs` for each
   subcommand. Note its `signature()` and `run()` shape.
2. Implement `run()` using `worker::SqlStorage::exec` instead of
   `rusqlite`.
3. Add the file with a module doc per `../CLAUDE.md` §3 (Mirrors line,
   `Used by:`, full divergence table).
4. Re-export from a new `stor/mod.rs`.
5. Register each `Stor*` in `src/cf/mod.rs::engine()::add_commands(...)`
   **AFTER** `add_custom_commands` so the shadow wins the name lookup.
6. Run the `stor` example via `mise run cf:dev` + curl; verify
   parity against desktop.
7. Update `../PORT_STATUS.md`'s shadow table to add the new rows.

### Things to NOT do

- **Do not** add empty/stub `.rs` files. A registered shadow that
  doesn't actually work is worse than no shadow -- the stock parse
  error tells the truth; a half-shadow lies.
- **Do not** register `Stor*` until each subcommand actually
  executes against `worker::SqlStorage` and produces correct output.
- **Do not** copy `nu-command::database`'s table abstractions
  wholesale -- they assume `rusqlite::Connection`. Use the
  `worker::SqlStorage` shape from `crates/cloudflare-shell-workspace/src/filesystem.rs`
  directly.

## Status until the port lands

- `stor` example is blocked on CF (documented in
  `CLOUDFLARE_STATUS.md`).
- This README is the canonical design plan.
- `../PORT_STATUS.md`'s shadow target table points at this README
  for the `stor` row.
