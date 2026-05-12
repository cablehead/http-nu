# `src/cf/shell/` -- Rust port of `@cloudflare/shell`

A Rust port of [`@cloudflare/shell`](https://www.npmjs.com/package/@cloudflare/shell)
targeting [`workers-rs`](https://github.com/cloudflare/workers-rs).
Schema-compatible with the JS package -- data written from either side
is readable by the other.

- **Live:** https://http-nu-cf.gedw99.workers.dev (serves
  `examples/cf-workspace-browser`, exercising the shadow command set).
- **Upstream package:** `@cloudflare/shell@0.3.6`
  (`.src/agents/packages/shell/`, gitignored).
- **Upstream tracking issue:**
  [cloudflare/workers-rs#998](https://github.com/cloudflare/workers-rs/issues/998).

## Layout

Filenames mirror the upstream JS package path-for-path so reviewers can
read both sides together.

```
src/cf/shell/
  README.md           <- you are here
  CLAUDE.md           working rules for editors (and Claude sessions)
  PORT.md             running ledger: what's ported, what's not, what diverges
  mod.rs              module entry + re-exports
  filesystem.rs       <- filesystem.ts (the Workspace class)
  schema.rs           SQL DDL (extracted; inline in filesystem.ts upstream)
  fs/
    mod.rs            module entry
    path_utils.rs     <- fs/path-utils.ts
```

Today only `filesystem.ts` (the `Workspace` class) and `fs/path-utils.ts`
are ported. Siblings (`fs/in-memory-fs.ts`, `backend.ts`, `memory.ts`,
`workspace.ts`, `prompt.ts`, `git/`, ...) are upstream-only. See `PORT_STATUS.md`
for the full coverage table.

## What's ported, what's not

`PORT_STATUS.md` has the canonical tables:

- **File-level:** which upstream files have a Rust sibling here.
- **Method-level:** every public method on `Workspace`, with upstream
  `filename:line` line refs, status, and deviation notes.
- **Type-level:** type mappings.
- **Schema compatibility:** which columns / CHECK constraints / R2 key
  shapes / constants must stay byte-identical with upstream.
- **Behavioral parity:** things that aren't just method presence --
  EISDIR on dir read, MIME type plumbing, MAX_PATH_LENGTH, POSIX error
  prefixes, etc.
- **Intentional deviations:** where we knowingly differ (`Ok(None)`
  for ENOENT instead of throwing, no D1 backend, `realpath` exposed
  publicly, etc.).
- **Next port targets:** ranked by leverage.

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

Open the upstream TS file at the cited line and diff the bodies. That
is the side-by-side review experience.

## Contributing

Read `CLAUDE.md`. It is the contributor checklist: file-layout rule,
provenance rule, schema interop contract, error-prefix convention, and
the "both targets compile" check that has to pass before any commit
lands.

## Build / test

```
# From repo root:
cargo check                                                            # desktop
CF_HANDLER_PATH=../../examples/cf-workspace-browser/serve.nu \
  cargo check --target wasm32-unknown-unknown --features cloudflare --no-default-features
```

For end-to-end Workers-side verification, see `CLOUDFLARE.md` ->
"Testing (desktop/CF parity)" for the `cf:dev` / `cf:deploy` curl
recipe.
