# Flaky test_parse_error_ansi_formatting Test

## Summary

`test_parse_error_ansi_formatting` is flaky due to a race condition between the
main thread and the logging handler thread.

## Symptoms

- Passes when run in isolation: `cargo test test_parse_error_ansi_formatting`
- Sometimes fails when run with full suite: `cargo test`
- Failure message: `stderr missing expected error text`
- stderr contains only the panic message, not the parse error

## Root Cause

Race condition in error event delivery:

```
Main Thread                          Handler Thread
    │                                     │
 1. parse_closure() fails                 │
 2. log_error() → emit(Event::Error)      │
 3. loop continues                        │
 4. rx.recv() returns None                │  (hasn't called blocking_recv yet)
 5. .expect() panics                      │
 6. process terminates ←──────────────────┘  event lost
```

The broadcast channel delivers the error event, but the handler thread may not
have called `blocking_recv()` before the main thread panics and terminates the
process.

## Affected Code

- `src/main.rs:159` - the `.expect()` that panics
- `src/engine.rs:315` - `log_error()` call in `script_to_engine`
- `src/logging.rs:496-499` - handler's `Event::Error` processing

## Potential Fixes

1. **Add small delay before panic** - gives handler time to process (hacky)
2. **Sync flush before exit** - ensure all events processed before terminating
3. **Direct stderr for fatal errors** - bypass broadcast for startup failures
4. **Accept flakiness** - the race window is small, test usually passes

## Status

Known issue, low priority. The error handling works correctly in practice—the
race only manifests in the specific test scenario where parse fails on the only
script provided via CLI argument.
