//! `sleep` shadow. Mirrors `nu-command/src/platform/sleep.rs`.
//!
//! Used by: `examples/basic/`, `examples/2048/`.
//!
//! Divergences from stock (against `nu-command` 0.112.1):
//!
//! | Stock feature                | Stock arg | Shadow?  | Notes |
//! |------------------------------|-----------|----------|-------|
//! | Required `duration: Duration`| yes       | parses   | Argument accepted; not honoured. |
//! | Rest `duration` (multi)      | yes       | parses   | Same. |
//! | **Actually pauses**          | yes       | **NO**   | Known, documented divergence. Scripts that depend on `sleep` for timing will behave wrong. |
//!
//! Wasm rationale: Workers' async event loop doesn't expose a sync
//! sleep, and Nu commands are sync; until we land an async Nu eval
//! path, the only honest options are (a) spin a busy-loop and burn
//! the Workers CPU budget or (b) yield-noop. We choose (b) and log
//! one warning per call so the script author knows.
//!
//! Scripts that loop with `sleep` (e.g. `examples/basic/`'s `/time`
//! route) will spin and trip the Workers CPU limit. Everything else
//! in the script parses and runs normally.

use std::cell::Cell;

use nu_engine::command_prelude::*;

thread_local! {
    /// Per-request count of sleep calls. Reset by `cf::mod.rs::fetch`
    /// at the start of each request. Defensive guard against runaway
    /// loops like `generate { sleep 1sec ... } true` -- since sleep
    /// is a no-op on CF, that pipeline spins until the Worker dies.
    /// After MAX_CALLS we error instead of no-op, breaking the loop.
    static SLEEP_CALLS: Cell<u32> = const { Cell::new(0) };
}

// Low cap so demos with infinite `generate { sleep 1sec ... } true`
// loops return quickly instead of streaming for 8+ seconds. The first
// few iterations are enough to demonstrate the streaming UX; anything
// more is a CF demo author asking to be killed.
const MAX_CALLS_PER_REQUEST: u32 = 64;

/// Called by the CF fetch handler at the top of each request to reset
/// the budget. Safe to call multiple times; idempotent.
pub fn reset_sleep_budget() {
    SLEEP_CALLS.with(|c| c.set(0));
}

#[derive(Clone, Default)]
pub struct Sleep;

impl Command for Sleep {
    fn name(&self) -> &str {
        "sleep"
    }
    fn signature(&self) -> Signature {
        Signature::build("sleep")
            .required("duration", SyntaxShape::Duration, "duration to sleep")
            .rest("rest", SyntaxShape::Duration, "additional durations")
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::Platform)
    }
    fn description(&self) -> &str {
        "(CF: NO-OP) Real sleep needs an async Nu eval path that does \
         not exist yet. Errors after 1024 calls per request to break \
         runaway generate-loops."
    }
    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let n = SLEEP_CALLS.with(|c| {
            let v = c.get() + 1;
            c.set(v);
            v
        });
        if n > MAX_CALLS_PER_REQUEST {
            return Err(ShellError::Generic(
                nu_protocol::shell_error::generic::GenericError::new(
                    format!(
                        "sleep called {n} times in this request -- aborting (CF no-op + generate-loop kills the worker)"
                    ),
                    "rewrite without `sleep` in a streaming generator on CF",
                    call.head,
                )
                .with_help(
                    "this is the CF defensive cap; on desktop sleep blocks and this loop would be fine",
                ),
            ));
        }
        // Only log the first few -- otherwise the no-op warning floods
        // the wrangler log on streaming generators.
        if n <= 3 {
            worker::console_warn!(
                "sleep called on CF target: no-op (call #{n}; capped at {MAX_CALLS_PER_REQUEST})"
            );
        }
        Ok(PipelineData::Empty)
    }
}
