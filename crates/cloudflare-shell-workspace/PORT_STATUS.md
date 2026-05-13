# `@cloudflare/shell` -> Rust port

Rust port of [`@cloudflare/shell@0.3.6`](https://www.npmjs.com/package/@cloudflare/shell)
(local clone: `.src/agents/packages/shell/`). The goal is byte-compatible
interop: data written from either side is readable by the other.

- **Target:** `cargo build --target wasm32-unknown-unknown --features cloudflare`
- **Depends on workers-rs:** `SqlStorage` (sync), `Bucket`, `DurableObject`, `State`
- **Upstream tracking issue:** [cloudflare/workers-rs#998](https://github.com/cloudflare/workers-rs/issues/998)

## File-level mapping

The port is split between two Rust crates' worth of code:

- **`cloudflare-shell`** (`crates/cloudflare-shell/`) -- backend-agnostic:
  `FileSystem` trait, shared types, `FsError`, `path_utils`, generic
  conformance suite. Independent crate; could be published or
  upstreamed to `workers-rs`.
- **`cloudflare-shell-workspace`** (this crate) -- wasm-only:
  `Workspace` impl (DO SQLite + R2) + its `schema`. Implements
  `cloudflare_shell::FileSystem`.

| Upstream (`.src/agents/packages/shell/src/`) | Here                                                  | Status                                                                                            |
|----------------------------------------------|-------------------------------------------------------|---------------------------------------------------------------------------------------------------|
| `fs/interface.ts`                            | `cloudflare-shell/src/interface.rs`                   | done -- `FileSystem` trait + Stat / EntryType / option types / WorkspaceChange* / constants       |
| `fs/path-utils.ts`                           | `cloudflare-shell/src/path_utils.rs`                  | done + `normalize_path` validator (length check)                                                  |
| `fs/in-memory-fs.ts` (744 lines)             | -                                                     | skip -- a desktop-only double would only catch divergence from itself, not from `Workspace`       |
| (port-only)                                  | `cloudflare-shell/src/error.rs`                       | done -- `FsError` enum w/ POSIX-prefixed `Display`; `From<worker::Error>` behind `workers` feature |
| (port-only)                                  | `cloudflare-shell/src/conformance.rs`                 | done -- generic `<F: FileSystem>` tests; run against `Workspace` via the consumer's harness        |
| `filesystem.ts` (1837 lines)                 | `cloudflare-shell-workspace/src/filesystem.rs`        | partial -- `Workspace` + `impl FileSystem for Workspace`; see method table                        |
| (inlined in `filesystem.ts`)                 | `cloudflare-shell-workspace/src/schema.rs`            | done -- SQL DDL extracted for Rust separation                                                     |
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
| `constructor` / `new`                         | 237  | 102    | done. Takes `(sql, r2, namespace)` instead of `WorkspaceOptions`; option bag pared back. Mirrors upstream's `VALID_NAMESPACE` (filesystem.ts:189) -- rejects anything not matching `/^[a-zA-Z][a-zA-Z0-9_]*$/`. Required: the namespace lands in `format!("cf_workspace_{ns}")` inline in SQL DDL/queries (SqlStorage can't parameterise table names), so this validation is the only line of defence against namespace-as-injection. |
| -- / `is_valid_namespace`                     | 189  | 80     | port-only helper. Iterative ASCII check (no `regex` dep). Tests: `valid_namespaces_accepted`, `invalid_namespaces_rejected`. |
| -- / `default`                                | -    | 151    | port-only convenience; uses `DEFAULT_NAMESPACE = "default"`.    |
| `exists`                                      | 1028 | 179    | done.                                                           |
| `fileExists`                                  | 1017 | 248    | done. Resolves symlinks like upstream; returns true only when the resolved row's `type = 'file'`. |
| `stat`                                        | 500  | 191    | done. Returns `Ok(None)` on ENOENT (TS: `Promise<FileStat \| null>`). |
| `lstat`                                       | 475  | 202    | done. Same `Ok(None)` semantics.                                |
| `readFile`                                    | 526  | 229    | done. EISDIR on dir; ENOENT -> `Ok(None)`.                      |
| `readFileBytes`                               | 569  | 241    | done. R2 spill resolved transparently. EISDIR on dir.           |
| `readFileStream`                              | 851  | 347    | done. R2 path proxies `ObjectBody::stream()` (workers-rs `ByteStream`); inline path wraps bytes in `stream::iter(once)` (Unpin-friendly, mirrors upstream's `enqueue + close`). EISDIR on dir; ENOENT -> `Ok(None)`. Return type `Option<ReadStream>` alias = `Pin<Box<dyn Stream<Item = Result<Vec<u8>>> + Unpin>>`. |
| `writeFile`                                   | 729  | 300    | done. Signature: `(path, content, mime_type: Option<&str>)`. None -> `'text/plain'`. EISDIR on root. |
| `writeFileBytes`                              | 611  | 314    | done. R2 spill at 1.5MB. `mime_type: Option<&str>`. None -> `'application/octet-stream'`. EISDIR on root. |
| `writeFileStream`                             | 907  | 449    | done (faithful). Drain stream into `Vec<u8>`, error `EFBIG` past `MAX_STREAM_SIZE` (100 MB), delegate to `write_file_bytes`. Stream-shaped in, single-shot out -- see "Behavioral parity" note. Cap is gated on the [`set_streaming_writes`](#workspace-method-level-mapping) toggle (OFF default = cap enforced; ON = cap lifted, forward-compat hook for the future multipart path). |
| -- / `set_streaming_writes` + `streaming_writes` | -    | 184 / 191 | port-only forward-compat toggle. OFF (default) keeps `write_file_stream` byte-faithful to upstream (cap enforced, collect-then-write). ON lifts the cap; the actual streaming-into-R2 path (R2 multipart upload, 5 MB parts) is the planned follow-up -- callers using ON today will benefit transparently when that lands. See "Intentional deviations". |
| `appendFile`                                  | 938  | 335    | done. Preserves existing `mime_type`.                           |
| `deleteFile`                                  | 990  | 861    | done. File/symlink only -- `EISDIR` on a directory ("use rm() instead"), matching upstream. Returns `Ok(false)` on ENOENT, `Ok(true)` on success; R2-backed rows have their object dropped via `rm_single`. |
| `readDir`                                     | 1041 | 471    | done. Names only.                                               |
| -- / `read_dir_with_file_types`               | -    | 481    | port-only. TS `readDir` returns `FileInfo[]`; we split for ergonomics. |
| `glob`                                        | 1071 | 813    | done.                                                           |
| `mkdir`                                       | 1100 | 513    | done. `MkdirOptions { recursive }` matches TS.                  |
| `rm`                                          | 1164 | 588    | done. `RmOptions { recursive, force }` matches TS.              |
| `cp`                                          | 1221 | 675    | done. Preserves source `mime_type`. `CpOptions { recursive }` matches TS.    |
| `mv`                                          | 1264 | 717    | done.                                                           |
| `symlink`                                     | 415  | 748    | done. `MAX_SYMLINK_DEPTH = 40` matches.                         |
| `readlink`                                    | 460  | 785    | done.                                                           |
| -- / `realpath`                               | -    | 805    | port-only public helper; TS resolves inline.                    |
| `diff`                                        | 1370 | -      | not ported (Agents-SDK structured editing).                     |
| `diffContent`                                 | 1390 | -      | not ported.                                                     |
| `getWorkspaceInfo`                            | 1406 | 1044   | done. Returns `WorkspaceInfo { file_count, directory_count, total_bytes, r2_file_count }`. Single `SUM(CASE ...)` scan over the index table, same query shape as upstream. |
| `onChange` (option callback, emitted at L312) | 108  | `set_on_change` + private `emit` | done. Setter is `set_on_change(&self, cb: OnChange)` (interior mutability via `Mutex`) rather than a constructor option -- callback type is `Arc<dyn Fn(WorkspaceChangeEvent) + Send + Sync>`. Emit sites wired into `write_inner` (Create/Update), `insert_dir` (Create on real insert), `symlink` (always Create), `rm_single` (Delete after DELETE). `cp` / `mv` / `append_file` inherit emits transitively. |
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
| `getWorkspaceInfo` return | 1407  | `WorkspaceInfo`                 | `{ file_count, directory_count, total_bytes, r2_file_count }`. Field names snake_case; types `u64` (TS: `number`). |

## Trait surface (`FileSystem`)

| Upstream (`fs/interface.ts`) | TS L | Rust (`crates/cloudflare-shell/src/interface.rs`) | Status                                              |
|-----------------------------|------|---------------------------------------------------|------------------------------------------------------|
| `readFile`                  | 53   | `read_file`                                       | done                                                 |
| `readFileBytes`             | 54   | `read_file_bytes`                                 | done                                                 |
| `writeFile`                 | 55   | `write_file`                                      | done (mime_type Option deviation)                    |
| `writeFileBytes`            | 56   | `write_file_bytes`                                | done                                                 |
| `appendFile`                | 57   | `append_file`                                     | done                                                 |
| `exists`                    | 58   | `exists`                                          | done                                                 |
| `stat`                      | 60   | `stat`                                            | done (Ok(None) deviation)                            |
| `lstat`                     | 62   | `lstat`                                           | done (Ok(None) deviation)                            |
| `mkdir`                     | 63   | `mkdir`                                           | done                                                 |
| `readdir`                   | 64   | `read_dir`                                        | done                                                 |
| `readdirWithFileTypes`      | 65   | `read_dir_with_file_types`                        | done                                                 |
| `rm`                        | 66   | `rm`                                              | done                                                 |
| `cp`                        | 67   | `cp`                                              | done                                                 |
| `mv`                        | 68   | `mv`                                              | done                                                 |
| `symlink`                   | 69   | `symlink`                                         | done                                                 |
| `readlink`                  | 70   | `readlink`                                        | done                                                 |
| `realpath`                  | 71   | `realpath`                                        | done                                                 |
| `resolvePath`               | 72   | `resolve_path` (default impl)                     | done. Pure path math; sync trait method with default delegating to `path_utils::resolve_path`. |
| `glob`                      | 73   | `glob`                                            | done                                                 |

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
  `set_on_change(&self, cb)` is functionally equivalent (the
  callback is per-instance state, fired after the same mutations) but
  fits a Rust call site that doesn't have a builder-style options bag.
  The `&self` signature uses interior mutability (`Mutex<Option<OnChange>>`)
  so callers don't need a `&mut Workspace` after construction.
- **`BufferEncoding` parameter is not surfaced on the API.** Upstream's
  `readFile`/`writeFile`/etc. accept an optional `encoding`
  (`"utf8" | "base64" | "hex" | "binary" | "latin1"`); see
  `fs/encoding.ts` (92 lines, not ported). We expose `read_file ->
  Option<String>` (UTF-8) and `read_file_bytes -> Option<Vec<u8>>`
  separately; callers do the base64/hex conversion at the boundary.
  **Not a compliance gap** -- the bytes on disk are identical; this is
  an API-surface deviation. Anyone porting JS callers needs to handle
  the conversion themselves (`base64`/`hex` crates on the Rust side).
- **`set_streaming_writes` toggle (port-only).** Gates
  `write_file_stream`'s cap. Upstream has no equivalent because
  upstream's `writeFileStream` *always* buffers and *always* enforces
  the 100 MB cap. The toggle exists so we can flip on a future
  streaming-into-R2 implementation (R2 multipart upload, ~5 MB peak
  memory) without an API break. **Today:** OFF preserves upstream
  parity; ON lifts the cap but still buffers (caller takes on the
  memory risk). **Future:** ON will switch to multipart upload; callers
  who already have ON will benefit transparently. Documented in the
  method-level table next to `writeFileStream`.
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

## `InMemoryFs`: not ported (deliberate)

Upstream ships `fs/in-memory-fs.ts` as a desktop-side double of
`Workspace` for tests / scratch buffers. We don't carry that here.
The reasons:

1. A desktop double tests itself, not `Workspace`. The conformance
   suite (`cloudflare_shell::conformance`) is generic over
   `<F: FileSystem>`; it must run against the real backend (DO
   SQLite + R2) to catch divergence. `cloudflare_shell_workspace::
   run_conformance` does exactly that.
2. Maintaining a 1500-line double in lockstep with `Workspace` is a
   permanent tax. We'd rather automate the wasm conformance route
   into CI than pretend a double provides parity.

If a second `FileSystem` impl becomes useful later (a JS-side Workers
shim, an R2-only impl), the generic conformance functions are ready
for it without modification.

## Next port targets (ranked)

1. **Stream-into-R2 path for `write_file_stream` (toggle ON).** Today
   ON only lifts the EFBIG cap -- bytes still land in a `Vec<u8>`
   before reaching R2. The actual memory win comes from switching to
   R2 multipart upload: `Bucket::create_multipart_upload`, then
   `upload_part` per 5 MB chunk, then `complete`. Peak memory drops to
   one part (~5 MB) regardless of total upload size. Already plumbed
   behind the `streaming_writes` toggle so callers don't change when
   this lands.
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
