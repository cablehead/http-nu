## Git Commit Style Preferences

**NEVER commit unless explicitly asked by the user.**

When committing: review `git diff`

- Use conventional commit format: `type: subject line`
- Keep subject line concise and descriptive
- **NEVER include marketing language, promotional text, or AI attribution**
- **NEVER add "Generated with Claude Code", "Co-Authored-By: Claude", or similar
  spam**
- Follow existing project patterns from git log
- Prefer just a subject and no body, unless the change is particularly complex

Example good commit messages from this project:

- `test: allow dead code in test utility methods`
- `fix: improve error handling`
- `feat: add a --fallback option to .static to support SPAs`
- `refactor: remove axum dependency, consolidate unix socket, tcp and tls handling`

## Tone and Communication

Prefer calm, matter-of-fact technical tone.

## Code Quality

Always run `./scripts/check.sh` before committing. Use `cargo fmt` to fix
formatting issues. Use ASCII characters only in code, comments, and documentation.

## Release Process

Use `/release [version]` command to execute the automated release workflow. See
`.claude/commands/release.md` for details.

<!-- ===========================================================================
     Sections below this marker are joeblew999-branch additions, NOT from
     cablehead/http-nu upstream. Keep them after upstream content so a merge
     from upstream is append-only and never produces a conflict on these
     lines. If upstream adds a section after their "Release Process", move
     this marker down -- never interleave.
     =========================================================================== -->

## CF Worker development workflow (joeblew999 branch)

When working on the Cloudflare Workers port (`src/cf/`, examples on CF,
`mise run cf:*` tasks). See `CLOUDFLARE.md` for full design.

1. **Iterate locally with `mise run cf:dev`** (~3s per change), not
   `cf:deploy` (~45s). First wasm build is slow; subsequent are fast.
   `console_log!` / `console_warn!` / panics print straight to the
   terminal -- no need for `cf:tail` against the deployed worker.

2. **Check `.src/` for prior art BEFORE writing greenfield code.** The
   `.src/` folder is gitignored and contains local clones of related
   libraries we mine for patterns. Grep first; guess second.
   - `.src/nu-on-web/` -- same Nu-on-wasm32 target, different host.
     Cargo features (`nu-command/js` + `rand`), shadow command patterns,
     JS bridge pattern. We pulled the `js` feature from there.
   - `.src/agents/packages/shell/` -- @cloudflare/shell schema +
     semantics. Our Workspace port in `src/cf/workspace/` mirrors this
     byte-for-byte so data is interoperable.
   - `.src/workers-rs/worker/src/` -- the Workers SDK we build on.
     Especially `sql.rs` (sync `SqlStorage`) and `durable.rs`.
   - Adapt patterns, don't copy verbatim -- host APIs differ between
     browser-wasm and Workers-wasm (e.g. sync `readFileSync` vs async
     R2). When a `.src/` pattern doesn't fit, note why before writing
     CF-specific code.

3. **All CF-only code lives under `src/cf/`.** Never edit upstream files
   (`src/lib.rs`, `src/handler.rs`, `src/commands.rs`, `src/response.rs`,
   etc.) for CF-specific reasons. Conflicts with cablehead/http-nu
   merges are the cost; this rule prevents them. The `Vfs` trait lives
   in `src/cf/vfs.rs` for the same reason -- when desktop ever opts
   into the same shadow surface, the trait promotes to a top-level
   `src/vfs.rs` THEN, not speculatively now.

4. **Nu commands that need shadowing on CF go in `src/cf/commands.rs`.**
   They route through the `Vfs` trait (filesystem) or stay
   self-contained for non-fs work. Before adding a new shadow, check
   whether enabling a `nu-command` feature (`js`, `rand`, etc.) would
   register the stock command. Stock command beats home-rolled shadow.

5. **The Workspace port lives at `src/cf/workspace/`.** Bug-for-bug
   schema compatibility with `@cloudflare/shell` is the contract --
   data written from the JS package must be readable here and vice
   versa. The async surface stays in `Workspace` (R2 spill); Nu eval
   gets a sync `SnapshotVfs` view that preloads what it needs and
   buffers writes for post-eval flush.

6. **R2 + DurableObject bindings live in `src/cf/wrangler.toml`.** Token
   for deploy is fetched from `fnox` by the `cf:deploy` mise task; you
   don't need to export it manually if you have fnox set up.

7. **Per-demo parity check is mandatory.** Each example must behave
   the same on desktop and CF -- they are the same Nu source. Workflow:

   ```
   # a) Desktop baseline
   mise run ex:<name>                          # serves at :3001
   curl -i http://127.0.0.1:3001/              # capture HTTP code, body, Content-Type

   # b) CF local (must match (a) before remote)
   CF_HANDLER_PATH=examples/<name>/serve.nu mise run cf:dev
   curl -i http://127.0.0.1:8787/              # diff against (a)

   # c) CF remote (only after (b) matches)
   CF_HANDLER_PATH=examples/<name>/serve.nu mise run cf:deploy
   curl -i https://http-nu-cf.gedw99.workers.dev/
   ```

   Don't claim a demo "works on CF" until (b) matches (a). If the
   behaviour diverges, fix the *cause* (commonly: a wasm-incompatible
   Nu command, `$env.PWD` path-resolution, a missing workspace file).
   Don't paper over by changing the example -- the demo is the spec.

   Exceptions are explicit and live in CLOUDFLARE.md's example status
   table: e.g. `sleep` is documented as a no-op on CF until async Nu
   eval lands; `path self` returns a workspace-rooted path (same
   semantic as desktop, different string). Anything else: parity.
