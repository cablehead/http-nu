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

`CLOUDFLARE.md` is the design doc (running state in
`CLOUDFLARE_STATUS.md`, subsystem rules in
`src/cf/{commands,shell}/CLAUDE.md`); this is the always-on checklist.

1. **Iterate with `mise run cf:dev`** (~3s/change), not `cf:deploy`
   (~45s). `console_log!` / `console_warn!` / panics print to the
   terminal -- no need for `cf:tail` against the deployed worker.
2. **Grep `.src/` BEFORE writing new wasm/CF code.** Local clones of
   prior art (nushell, nu-on-web, @cloudflare/shell, workers-rs, ...);
   see `CLOUDFLARE.md` Acknowledgements for what each provides.
3. **CF-only code lives under `src/cf/`. For shared concerns spanning
   desktop AND wasm, prefer a top-level abstraction over cfg gates in
   upstream files.** Two shapes today:
   - **In-tree trait** (today: `crate::vfs::Vfs`). Trait + desktop
     impl at `src/<thing>.rs` (gated `cfg(feature = "desktop")`);
     wasm impl at `src/cf/<thing>.rs`. Upstream files call the trait
     unconditionally -- no per-call-site cfg gates.
   - **Workspace crate** (today: `cloudflare-shell` +
     `cloudflare-shell-workspace`). Same idea but the trait lives in
     its own crate so it can be reused outside this project. Use this
     shape when the abstraction has value beyond http-nu.

   Cfg gates remain appropriate ONLY for things without a useful
   abstraction (`notify` vs DO alarm, etc.).
4. **Shadow commands mirror Nu's source tree path-for-path:**
   `src/cf/nu/nu_command/<cat>/<name>.rs` <-> `nu-command/src/<cat>/<name>.rs`.
   Check whether a `nu-command` feature would register the stock
   command before shadowing. Full rules: `src/cf/nu/nu_command/CLAUDE.md`.
5. **`@cloudflare/shell` Rust port lives in two workspace crates:**
   - `crates/cloudflare-shell/` -- backend-agnostic trait + types +
     conformance suite. Reusable from any Rust project.
   - `crates/cloudflare-shell-workspace/` -- wasm-only Workspace
     impl (DO SQLite + R2). Schema-compatible; bidirectional interop
     is the contract. Filenames mirror the upstream JS package
     path-for-path (`filesystem.ts -> filesystem.rs` etc.). Full
     rules: each crate's `CLAUDE.md`.
6. **R2 + DO bindings live in `src/cf/wrangler.toml`.** `cf:deploy`
   pulls `CLOUDFLARE_API_TOKEN` from `fnox`.
7. **Per-demo desktop/CF parity check is mandatory** before claiming
   a demo works on CF. Recipe: see `CLOUDFLARE.md` "Testing
   (desktop/CF parity)". Fix the cause, not the example.
