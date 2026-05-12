# `src/cf/nu/nu_command/` -- Nu shadow commands for the CF target

Drop-in replacements for stock `nu-command` builtins that either can't
compile to `wasm32-unknown-unknown`, can't reach a real filesystem under
Workers, or need to be redirected through the workspace `Vfs` instead of
`std::fs`.

Same concept as `src/cf/shell/` -- but for Nu, not `@cloudflare/shell`.
The shell port is a Rust translation of a JS package; this directory is a
set of Nu `Command` impls that *replace* upstream Nu commands at engine
init time. Different mechanic, same discipline: file-for-file parity with
upstream so a reviewer can diff bodies side-by-side.

- **Live:** https://http-nu-cf.gedw99.workers.dev (the
  `cf-workspace-browser` example exercises this command set).
- **Upstream package:** `nu-command` from
  [`nushell/nushell`](https://github.com/nushell/nushell)
  (`.src/nushell/crates/nu-command/`, gitignored).

## Layout

Filenames mirror `nu-command/src/<category>/<name>.rs` path-for-path.
TS-to-Rust transliteration is irrelevant here; both sides are already
Rust, so the mirror is literal.

```
src/cf/nu/nu_command/
  README.md                  <- you are here
  mod.rs                     module entry + re-exports
  shared.rs                  Vfs path normalisation + error helpers
  filesystem/                <- nu-command/src/filesystem/
    cp.rs   glob.rs  ls.rs   mkdir.rs  mv.rs
    open.rs rm.rs    save.rs
  path/                      <- nu-command/src/path/
    exists.rs  self_.rs
  platform/                  <- nu-command/src/platform/
    sleep.rs
```

The mapping rule (also stated in `mod.rs`):

```
nu-command/src/filesystem/ls.rs      ->  src/cf/nu/nu_command/filesystem/ls.rs
nu-command/src/path/exists.rs        ->  src/cf/nu/nu_command/path/exists.rs
nu-command/src/platform/sleep.rs     ->  src/cf/nu/nu_command/platform/sleep.rs
```

When Nu adds or restructures a stock command we want to shadow, add or
move the equivalent file here in the same relative path. Process
discipline; the compiler doesn't enforce it.

## The structural picture: BASE + LEAF (don't forget this)

The reason this directory stays small is not luck. It's a deliberate
design: **fix the base abstraction once, get every consumer for free.**
Nushell splits naturally into three layers; only the bottom one needs
real work from us.

```
LAYER 1 -- BASE (compiles to wasm; "just works" once enabled)
  nu-protocol         types, signatures, errors
  nu-engine           eval loop
  nu-parser           parser
  nu-cmd-lang         language primitives
  nu-cmd-extra        extra core commands
  nu-std              Nu-side standard library (written in Nu)
  nu-utils            small helpers

LAYER 2 -- FREE under `nu-command/js` (already enabled by our `cloudflare` feature)
  path join / dirname / basename            no OS lookup
  date now / format date / date humanize    chrono with JS time source
  random integer/float/bool/chars/uuid/...  RNG with JS seed
  ALL pure-data ops: from/to json, where, each, sort-by,
  group-by, select, update, math, str, list, record,
  conversions, formats ...                   no OS dependency

LAYER 3 -- LEAF (this directory; the only work we do)
  ls / open / save / mkdir / rm / cp / mv / glob   route through Vfs
  path self                                          wasm-broken Path::is_absolute
  sleep                                              sync Nu, no async yield
```

Once Layer 3 is in place, **everything in Layers 1 and 2 inherits it
automatically.** That includes:

- **All pure-data commands** -- they don't touch the FS at all, so they
  cross to wasm without effort.
- **Anything in `nu-std`** (Nu's stdlib, written in Nu) -- when a stdlib
  function does `ls foo | where ...`, Nu's parser resolves `ls` through
  the command registry at parse time. Our `VfsLs` wins the name lookup,
  so the stdlib function gets Vfs without us touching it.
- **User scripts and examples** -- same name-lookup mechanism. A script
  that calls `open` calls our shadow; pipelines composed of shadowed
  commands chain cleanly.

So the leverage isn't in *adding more shadows* -- it's in keeping the
Layer 3 set tight and well-tested. The demand map in
[`PORT_STATUS.md`](PORT_STATUS.md) is the canonical evidence that
this strategy holds: only ~14 commands across all examples touch
Layer 3 at all.

### The one caveat

A stock command implemented in Rust that calls `std::fs::*` *directly*
(rather than via Nu's name lookup) bypasses our shadow. It will
compile to wasm, then crash at runtime. Examples are commands like
`du`, `which`, `ps`, `sys`. The defences are:

1. **Demand-driven inaction.** If no example uses the command, don't
   shadow it -- and the runtime crash never happens.
2. **`nu-command` feature flags.** Upstream gates many of these
   commands behind features (`os`, `network`, `sqlite`). Leaving the
   feature off means the command isn't even compiled.
3. **Add a Vfs-routed shadow** if/when an example needs it.

So Layer 3 isn't a closed list -- it can grow -- but it grows
*only* in response to demand from a real example.

## What's shadowed and why

Three reasons a stock command gets a shadow here:

1. **Routes through `Vfs`.** Filesystem commands (`ls`, `open`, `save`,
   `cp`, `mv`, `rm`, `mkdir`, `glob`) must hit the workspace SQLite/R2
   backend, not `std::fs`. Workers has no disk.
2. **Stock command parse-errors on wasm.** E.g. `path self` calls
   `engine_state.cwd()` which goes through `Path::is_absolute()` --
   always false on `wasm32-unknown-unknown` -- and aborts at parse time.
   See `path/self_.rs` for the full story.
3. **Stock command would burn the Workers CPU budget.** E.g. `sleep`
   has no async path in sync Nu commands; the shadow is a logged no-op
   rather than a busy-loop. See `platform/sleep.rs`.

What's **not** shadowed: anything that just works under the `js` feature
of `nu-command`. `date now`, `format date`, `random integer` and friends
come from upstream directly (see Cargo.toml's `cloudflare` feature).
Check whether a `nu-command` feature flag would register the stock
command before adding a shadow.

## Reading the code

Every shadow file leads with a module-level comment pinning it to its
upstream sibling and (if relevant) explaining the deviation:

```rust
//! `ls` shadow. Mirrors `nu-command/src/filesystem/ls.rs`.

//! `sleep` shadow. Mirrors `nu-command/src/platform/sleep.rs`.
//!
//! CF target: NO-OP. Workers' async event loop doesn't expose a sync
//! sleep, and Nu commands are sync; ...
```

Open the upstream file at the cited path and diff. Same review
experience as `src/cf/shell/`.

## Build / test

```
# From repo root:
cargo check                                                            # desktop
CF_HANDLER_PATH=../../examples/cf-workspace-browser/serve.nu \
  cargo check --target wasm32-unknown-unknown --features cloudflare --no-default-features
```

Both targets must compile after any edit here. End-to-end Workers-side
verification: see `CLOUDFLARE.md` -> "Testing (desktop/CF parity)" for
the `cf:dev` / `cf:deploy` curl recipe.
