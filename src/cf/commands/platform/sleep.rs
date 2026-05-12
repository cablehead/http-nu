//! `sleep` shadow. Mirrors `nu-command/src/platform/sleep.rs`.
//!
//! CF target: NO-OP. Workers' async event loop doesn't expose a sync
//! sleep, and Nu commands are sync; until we land an async Nu eval
//! path, the only honest options are (a) spin a busy-loop and burn
//! the Workers CPU budget or (b) yield-noop. We choose (b) and log
//! one warning per call so the script author knows.
//!
//! Scripts that loop with `sleep` (e.g. examples/basic.nu's /time
//! route) will spin and trip the Workers CPU limit. Everything else
//! in the script parses and runs normally.

use nu_engine::command_prelude::*;

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
         not exist yet."
    }
    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        _call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        worker::console_warn!(
            "sleep called on CF target: no-op (await yields aren't reachable from sync Nu commands)"
        );
        Ok(PipelineData::Empty)
    }
}
