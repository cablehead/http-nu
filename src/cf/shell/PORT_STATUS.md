# `@cloudflare/shell` -> Rust port

Rust port of [`@cloudflare/shell@0.3.6`](https://www.npmjs.com/package/@cloudflare/shell)
(local clone: `.src/agents/packages/shell/`). The goal is byte-compatible
interop: data written from either side is readable by the other.

- **Target:** `cargo build --target wasm32-unknown-unknown --features cloudflare`
- **Depends on workers-rs:** `SqlStorage` (sync), `Bucket`, `DurableObject`, `State`
- **Upstream tracking issue:** [cloudflare/workers-rs#998](https://github.com/cloudflare/workers-rs/issues/998)

## File-level mapping

The port is split between two Rust crates' worth of code:

- **`src/shell/`** -- backend-agnostic: `FileSystem` trait, shared
  types, `FsError`, `InMemoryFs`, `path_utils`, conformance tests.
  Compiles on desktop AND wasm.
- **`src/cf/shell/`** -- wasm-only: `Workspace` impl (DO SQLite + R2)
  + its `schema`. Implements `crate::shell::FileSystem`.

| Upstream (`.src/agents/packages/shell/src/`) | Here                                  | Status                                                                                            |
|----------------------------------------------|---------------------------------------|---------------------------------------------------------------------------------------------------|
| `fs/interface.ts`                            | `src/shell/interface.rs`              | done -- `FileSystem` trait + Stat / EntryType / option types / WorkspaceChange* / constants       |
| `fs/path-utils.ts`                           | `src/shell/path_utils.rs`             | done + `normalize_path` validator (length check)                                                  |
| `fs/in-memory-fs.ts` (744 lines)             | `src/shell/in_memory_fs.rs`           | partial -- core FS methods + `impl FileSystem`; lazy entries / `chmod` / `utimes` / `link` deferred |
| (port-only)                                  | `src/shell/error.rs`                  | done -- `FsError` enum w/ POSIX-prefixed `Display`; `From<worker::Error>` on wasm                  |
| (port-only)                                  | `src/shell/conformance.rs`            | done -- generic `<F: FileSystem>` tests; the keystone of the mock-divergence defence              |
| `filesystem.ts` (1837 lines)                 | `src/cf/shell/filesystem.rs`          | partial -- `Workspace` + `impl FileSystem for Workspace`; see method table                        |
| (inlined in `filesystem.ts`)                 | `src/cf/shell/schema.rs`              | done -- SQL DDL extracted for Rust separation                                                     |
| `fs/encoding.ts`                             | -                                     | TBD                                                                                               |
| `backend.ts` (`StateBackend`)                | -                                     | skip unless agents-SDK integration                                                                |
| `memory.ts` (`FileSystemStateBackend`)       | -                                     | skip unless agents-SDK integration                                                                |
| `workspace.ts` (`WorkspaceFileSystem` wrapper) | -                                   | skip unless agents-SDK integration                                                                |
| `prompt.ts`                                  | -                                     | skip -- LLM prompt scaffolding                                                                    |
| `helpers.ts`                                 | -                                     | audit-first                                                                                       |
| `extras.ts`                                  | -                                     | audit-first                                                                                       |
| `workers.ts`                                 | -                                     | audit-first                                                                                       |
| `git/fs-adapter.ts`                          | -                                     | TBD -- needed for `git pull` <-> Workspace                                                        |
| `git/index.ts`                               | -                                     | TBD                                                                                               |
| `git/provider.ts`                            | -                                     | TBD                                                                                               |

## `Workspace` method-level mapping

`filesystem.ts` class `Workspace` (L223) -> `filesystem.rs` struct
`Workspace` (L100). All `async` methods upstream; Rust `pub async fn`
here. Line numbers are anchors for side-by-side review.

| Method (TS / Rust)                            | TS L | Rust L | Status / deviation                                              |
|-----------------------------------------------|------|--------|-----------------------------------------------------------------|
| `constructor` / `new`                         | 237  | 110    | done. Takes `(sql, r2, namespace)` instead of `WorkspaceOptions`; option bag pared back. |
| -- / `default`                                | -    | 126    | port-only convenience; uses `DEFAULT_NAMESPACE = "default"`.    |
| `exists`                                      | 1028 | 153    | done.                                                           |
| `fileExists`                                  | 1017 | -      | not ported. Use `exists` + `stat`.                              |
| `stat`                                        | 500  | 164    | done. Returns `Ok(None)` on ENOENT (TS: `Promise<FileStat \| null>`). |
| `lstat`                                       | 475  | 173    | done. Same `Ok(None)` semantics.                                |
| `readFile`                                    | 526  | 198    | done. EISDIR on dir; ENOENT -> `Ok(None)`.                      |
| `readFileBytes`                               | 569  | 208    | done. R2 spill resolved transparently. EISDIR on dir.           |
| `readFileStream`                              | 851  | -      | not ported. Would need worker-rs streaming body bridge.         |
| `writeFile`                                   | 729  | 258    | done. Signature: `(path, content, mime_type: Option<&str>)`. None -> `'text/plain'`. EISDIR on root. |
| `writeFileBytes`                              | 611  | 262    | done. R2 spill at 1.5MB. `mime_type: Option<&str>`. None -> `'application/octet-stream'`. EISDIR on root. |
| `writeFileStream`                             | 907  | -      | not ported (pair with `readFileStream`).                        |
| `appendFile`                                  | 938  | 273    | done. Preserves existing `mime_type`.                           |
| `deleteFile`                                  | 990  | -      | not ported. Use `rm` (covers files and dirs).                   |
| `readDir`                                     | 1041 | 365    | done. Names only.                                               |
| -- / `read_dir_with_file_types`               | -    | 372    | port-only. TS `readDir` returns `FileInfo[]`; we split for ergonomics. |
| `glob`                                        | 1071 | 656    | done.                                                           |
| `mkdir`                                       | 1100 | 401    | done. `MkdirOptions { recursive }` matches TS.                  |
| `rm`                                          | 1164 | 461    | done. `RmOptions { recursive, force }` matches TS.              |
| `cp`                                          | 1221 | 538    | done. Preserves source `mime_type`. `CpOptions { recursive }` matches TS.    |
| `mv`                                          | 1264 | 574    | done.                                                           |
| `symlink`                                     | 415  | 602    | done. `MAX_SYMLINK_DEPTH = 40` matches.                         |
| `readlink`                                    | 460  | 634    | done.                                                           |
| -- / `realpath`                               | -    | 650    | port-only public helper; TS resolves inline.                    |
| `diff`                                        | 1370 | -      | not ported (Agents-SDK structured editing).                     |
| `diffContent`                                 | 1390 | -      | not ported.                                                     |
| `getWorkspaceInfo`                            | 1406 | -      | not ported (metadata helper).                                   |
| `onChange` (option callback, emitted at L312) | 108  | `set_on_change` + private `emit` | done. Setter is `set_on_change(&mut self, cb: OnChange)` rather than a constructor option -- callback type is `Arc<dyn Fn(WorkspaceChangeEvent) + Send + Sync>`. Emit sites wired into `write_inner` (Create/Update), `insert_dir` (Create on real insert), `symlink` (always Create), `rm_single` (Delete after DELETE). `cp` / `mv` / `append_file` inherit emits transitively. |
| `SqlBackend.query` / `.run` (raw SQL)         | 38/42| -      | not ported as `Workspace` methods; callers use `worker::SqlStorage` directly. |

## Type-level mapping

| Upstream type             | TS L  | Our equivalent                  | Notes                                                              |
|---------------------------|-------|---------------------------------|--------------------------------------------------------------------|
| `SqlParam`                | 31    | -- (uses `worker::SqlStorage` natively) | We don't re-define; `SqlStorage::exec` consumes params natively. |
| `SqlBackend`              | 37    | --                              | Not ported; only `worker::SqlStorage` is targeted today.           |
| `WorkspaceOptions`        | 96    | constructor args + `set_on_change` | We take `(sql, r2, namespace)`; `onChange` callback is attached post-construction via `set_on_change`. Other options (`r2Prefix`, `inlineThreshold`, `name`) aren't surfaced yet. |
| `EntryType`               | 120   | `EntryType` (filesystem.rs)     | `File`/`Directory`/`Symlink`. SQL stores lowercased strings.       |
| `FileInfo` / `FileStat`   | 122/133 | `Stat`, `DirEntry`            | Two structs vs TS type alias. Field names snake_case. `Stat` includes a `mode: u32` field computed from `kind` (not stored in DB). |
| `WorkspaceChangeType`     | 135   | `WorkspaceChangeType`           | Enum `Create`/`Update`/`Delete` matches TS string union.            |
| `WorkspaceChangeEvent`    | 137   | `WorkspaceChangeEvent`          | Field `kind` instead of TS reserved `type`; `entry_type` snake_cased. |
| `OnChange`                | -     | `OnChange`                      | Port-only alias `Arc<dyn Fn(WorkspaceChangeEvent) + Send + Sync>` so callers (Rust + future cross-DO proxies) can pass listeners cheaply. |
| `WorkspaceFsLike`         | 162   | --                              | Pick<> shape for callers; Rust callers use concrete `Workspace`.   |

## Schema compatibility

Byte-compatible with `@cloudflare/shell@0.3.6`:

| Aspect                     | Value                                            |
|----------------------------|--------------------------------------------------|
| Table name pattern         | `cf_workspace_<namespace>`                       |
| Index name pattern         | `idx_<table>_parent_path`                        |
| Default namespace          | `"default"`                                      |
| `path` column              | `TEXT PRIMARY KEY`                               |
| `parent_path` column       | `TEXT NOT NULL`                                  |
| `name` column              | `TEXT NOT NULL`                                  |
| `type` CHECK               | `'file' \| 'directory' \| 'symlink'`             |
| `mime_type` default        | `'text/plain'`                                   |
| `size` default             | `0`                                              |
| `storage_backend` CHECK    | `'inline' \| 'r2'`                               |
| `content_encoding` default | `'utf8'` (binary writes flag as `'base64'`)      |
| R2 key shape               | `${bucket_prefix}/${namespace}<path>`            |
| R2 spill threshold         | `1_500_000` bytes                                |
| `MAX_SYMLINK_DEPTH`        | 40                                               |
| `MAX_PATH_LENGTH`          | 4096                                             |
| File mode (`Stat.mode`)    | `0o644` file / `0o755` dir / `0o777` symlink     |

DDL canonical source: `schema.rs::create_table_sql` /
`create_index_sql`. Compare against the SQL strings in upstream
`filesystem.ts`'s init paths.

Mode bits are computed at read time (`Stat::mode_for`), not stored in
the DB -- matches upstream `@cloudflare/shell` (modes are computed at
the `FileSystem` interface boundary).

## Behavioral parity

These behavioral details all match upstream now. Listed here so the
review doesn't have to re-derive them from the code:

- **`writeFile` / `writeFileBytes` accept `mime_type: Option<&str>`.**
  None defaults to `text/plain` / `application/octet-stream` matching
  TS positional defaults. Mime is persisted in the row's `mime_type`
  column.
- **`appendFile` preserves the existing entry's `mime_type`** (re-reads
  via `lstat` rather than overwriting with the default).
- **`cp` preserves source `mime_type`** (matches upstream
  filesystem.ts:1255).
- **`/_workspace/put` debug route honors request `Content-Type`** so
  browser uploads land with the right `mime_type` and serve back via
  `.static` correctly.
- **EISDIR on root write.** `writeFileBytes("/")` / `writeFile("/")`
  return `Err("EISDIR: cannot write to root directory")` (upstream
  filesystem.ts:619 / L737).
- **EISDIR on read-of-directory.** `read_file_bytes` / `read_file`
  return `Err("EISDIR: <path> is a directory")` when called on a dir
  (upstream filesystem.ts:544 / L587). ENOENT still maps to `Ok(None)`.
- **ENAMETOOLONG enforced.** `MAX_PATH_LENGTH = 4096`; checked via
  `normalize_path` at every public method entry. Symlink target length
  is also bounded.
- **POSIX error-prefix convention.** Every error string starts with a
  POSIX errno-style prefix (`ENOENT:`, `EISDIR:`, `ENAMETOOLONG:`,
  `ENOTEMPTY:`, `ELOOP:`, `EILSEQ:`, `EIO:`, `ENOSPC:`). Callers can
  pattern-match on the prefix for error classification.

## Intentional deviations

- **`Ok(None)` instead of throwing on ENOENT** for read-side methods
  (`stat`, `lstat`, `read_file*`, `readlink`, `read_dir*`, `realpath`).
  Upstream returns `Promise<T | null>` for the same cases; mapping `null
  -> None` is the Rust-idiomatic equivalent. Note: EISDIR (read on a
  directory) is still an `Err`, matching upstream -- only ENOENT is
  Ok(None).
- **No D1 backend.** Upstream's `SqlSource` is `SqlStorage | D1Database
  | SqlBackend`. We hardcode `worker::SqlStorage` because it's the only
  source with a sync `exec` -- D1 is async, and our async wrapper would
  collapse if D1 needs `.await` inside what is otherwise a thin
  sync-bridge. D1 path is a future variant.
- **`onChange` is a post-construction setter, not a constructor option.**
  Upstream `WorkspaceOptions.onChange` is passed at construction;
  `set_on_change(&mut self, cb)` is functionally equivalent (the
  callback is per-instance state, fired after the same mutations) but
  fits a Rust call site that doesn't have a builder-style options bag.
- **`realpath` exposed publicly.** Upstream resolves symlinks inline
  inside methods that need it; we surface a public helper because
  callers (e.g. `SnapshotVfs`) want the resolved path directly.
- **`read_dir_with_file_types` split out of `read_dir`.** Names-only is
  the common path; `FileInfo[]` variant is opt-in.
- **`ensureInit()` pattern.** Upstream `await this.ensureInit()` at the
  top of every method; we init eagerly in `new()`. Functionally
  equivalent once the constructor returned.

## What's load-bearing from workers-rs

- **`SqlStorage` with sync `exec`** -- the whole port hinges on this.
  R2 calls stay `async`; SQL ops stay sync; the `async fn` signatures
  on `Workspace` exist for compositional convenience, not because
  `SqlStorage` forced us into them.
- **`Bucket`** -- R2 binding for spill.
- **`DurableObject` + `State`** -- per-user isolate. The Workspace
  binds to `state.storage().sql()`.

No worker-rs gaps blocked the port. The sync `SqlStorage::exec` API is
what makes the bridge possible from inside a DO without an async
runtime.

## `InMemoryFs` port notes

Implemented as a drop-in alternative to `Workspace` for tests / scratch
buffers. Same method surface (`read_file*`, `write_file*`,
`append_file`, `stat`, `lstat`, `exists`, `mkdir`, `read_dir*`, `rm`,
`cp`, `mv`, `symlink`, `readlink`, `realpath`, `glob`), same
`Ok(None)`-on-ENOENT convention, same `Stat` / `DirEntry` / option
types, same `set_on_change` listener wiring (port-only -- upstream
`InMemoryFs` does not emit; we add it for API symmetry with `Workspace`
so test code can subscribe regardless of backend).

Storage is a flat `HashMap<String, Node>` keyed by absolute path rather
than upstream's `VDirNode` tree. Semantic parity, simpler representation
(`read_dir` becomes a prefix filter on the map; lookups are O(1)).

**Not ported** (deferred -- track here):
- **Lazy file entries** (`LazyFileProvider`, `forceLazy`). Useful for
  test fixtures that compute content on demand; not on our critical
  path.
- **Sync helpers** (`writeFileSync`, `mkdirSync`, `writeFileLazy`). Our
  `async fn` surface covers the same ground.
- **`chmod` / `utimes` / `link`**. Modes are computed at read time
  (matches upstream `Workspace`); explicit chmod/utimes aren't used by
  any caller yet.
- **`InitialFiles` constructor option**. `new()` returns an empty FS;
  callers chain `write_file_bytes(...)` to seed.

## Next port targets (ranked)

1. **`readFileStream` / `writeFileStream`** -- streaming I/O on top of
   `worker::Response::from_stream`. Pair with eventual `.static` Range
   support.
2. **`git/` (3 files)** -- isomorphic-git fs adapter. Either bridge
   isomorphic-git via `wasm_bindgen` or port the bits we need to
   `gitoxide`. Days of work; only when we actually want `git pull`
   against Workspace.
3. **CI-automate the conformance route.** The wasm harness at
   `GET /<user>/_workspace/conformance` is wired but invoked manually
   today via `mise run cf:dev` + curl. A `mise run cf:conformance`
   task that brings up wrangler dev, hits the route, and asserts a
   200 would close the loop into CI.

Agents-SDK glue (`backend.ts`, `memory.ts`, `workspace.ts`,
`prompt.ts`) is intentionally not on this list -- not relevant unless
we're building agent state on CF.
