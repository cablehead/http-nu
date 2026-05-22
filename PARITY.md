# Parity: vt100 -> wezterm-term

Goal: replace `vt100::Parser` with `wezterm_term::Terminal` in `src/pty.rs`
without changing the observable behavior of `pty open`/`write`/`resize`/
`stream`/`close`/`list`/`meta`. This file defines what "parity" means and
how each criterion is checked.

## Out of scope (intentional)

- Snapshot byte format does NOT have to be byte-identical to vt100's
  `state_formatted()` output. The contract is: the snapshot when written
  to a fresh xterm-compatible terminal reproduces the current screen,
  cursor, and SGR state. ghostty-web in the browser is the consumer.
- Performance: not measured here. Target is "not obviously worse."
- New features wezterm-term enables (sixel, kitty graphics, OSC 8) are
  bonus; parity does not require exercising them.

## Status

All testable parity criteria pass on `pty-wezterm-term` and `pty` branches.
Tests: `tests/pty_smoke.nu`, `tests/pty_parity.nu`.

Browser end-to-end (manual) is the remaining unchecked criterion.

## Build parity

- [ ] `cargo build` succeeds on the new branch.
- [ ] `cargo build --release` succeeds.
- [ ] No new clippy errors introduced (warnings OK if pre-existing).

## Test-suite parity

The existing test suite has no pty-specific tests, so the bar is:

- [ ] `cargo test` passes on `pty` branch (baseline).
- [ ] `cargo test` passes on `pty-wezterm-term` branch.
- [ ] Diff in test results between branches: zero.

## Behavioral parity (functional)

Each item must work the same as on the `pty` branch.

### B1. pty open (exec mode)
- `pty open bash` returns a sid.
- Bash starts and produces a prompt (visible via `pty stream`).
- `bus` publishes `pty.events: {event: created, sid, cols, rows}`.

### B2. pty open --embedded
- `pty open --embedded` returns a sid.
- Embedded nu REPL starts and responds to `1 + 1` returning `2`.

### B3. pty write
- Bytes piped to `pty write $sid` reach the slave's stdin.
- `ls\n` typed into a bash session produces output.

### B4. pty resize
- After `pty resize $sid 120 30`, the slave sees the new size via SIGWINCH.
- Verified by running `stty size` after a resize and seeing the new dims.

### B5. pty stream snapshot-on-attach (the killer feature)
- Open a session, run a TUI app that paints the screen (e.g. `vim`).
- Detach (drop the SSE consumer).
- Re-attach via `pty stream`.
- The first bytes received reconstruct the current screen, cursor pos,
  and color state. (Verified by writing the bytes to a fresh xterm and
  comparing to the live terminal -- visual equivalence, not bytewise.)

### B6. pty stream last-attach-wins
- Attach client A. Attach client B. A's stream closes (returns Ok(false));
  B starts receiving from the snapshot onward.

### B7. DA1/DA2 auto-reply when no client attached
- Open `pty open bash`, do NOT attach a stream.
- Bash's reedline (or fish) issues DA1; replies must come back via the
  writer so the shell unblocks.
- Verified by: open session, send `\x1b[c` via pty write, observe that
  the slave's tty gets `\x1b[?62;22c` back (test program echoes its
  stdin). On the `pty` branch this is done by `scan_da_queries`; on
  the wezterm-term branch this is done natively by Terminal's writer.

### B8. pty close
- `pty close $sid` kills the child, drops fds, publishes `deleted` event.
- Session is gone from `pty list`.

### B9. pty list
- Returns rows for all open sessions with sid + meta.

### B10. pty meta get/set
- `pty meta set $sid foo "bar"` then `pty meta get $sid foo` returns "bar".

### B11. Death reaper
- Spawn `pty open bash`, then exit bash from inside the pty.
- Reader thread sees EOF, removes session, publishes `died` event.

## Browser end-to-end (manual)

Run a build of http-nu-pty pty-wezterm-term against ghostty-web-nu's
`serve-sessions.nu` and verify in a real browser:

- [ ] Open sessions UI; click "new session".
- [ ] See the embedded nu REPL prompt.
- [ ] Type a command (e.g. `ls`); see output.
- [ ] Resize browser window; vim/htop inside the session reflows.
- [ ] Refresh the browser tab; the screen state is preserved (snapshot).
- [ ] Run `cat /etc/services` (lots of output); no drops or hangs.
- [ ] Close session via UI; bus event fires; session disappears from list.

## Checking procedure

For each numbered behavioral item, prefer a small repeatable script over
manual verification where possible. End-to-end browser test stays manual.

Pass = green box on every item above. No partial credit.
