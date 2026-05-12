//! `path self` shadow. Mirrors `nu-command/src/path/self_.rs`.
//!
//! Stock `path self` calls `engine_state.cwd()` -> `AbsolutePathBuf::try_from($env.PWD)`
//! -> `std::path::Path::is_absolute()` on wasm32-unknown-unknown, which
//! returns false for every path. So the stock command unconditionally
//! errors at parse-time on CF.
//!
//! This shadow returns a workspace-rooted path instead of a disk-rooted
//! one. Same semantic as stock ("the script's location"), different
//! string. Demos using `path self | path dirname | path join assets`
//! get a usable workspace path (e.g. `/handler.nu` -> dirname `/` ->
//! join `assets` = `/assets`).
//!
//! Marked `is_const = true` so it works inside `const x = path self`
//! same as stock.

use nu_engine::command_prelude::*;
use nu_protocol::engine::StateWorkingSet;

#[derive(Clone, Default)]
pub struct VfsPathSelf;

impl Command for VfsPathSelf {
    fn name(&self) -> &str {
        "path self"
    }

    fn signature(&self) -> Signature {
        Signature::build("path self")
            .input_output_type(Type::Nothing, Type::String)
            .allow_variants_without_examples(true)
            .optional(
                "path",
                SyntaxShape::Filepath,
                "path to join onto the script root",
            )
            .category(Category::Path)
    }

    fn description(&self) -> &str {
        "Workspace-rooted equivalent of stock `path self` for the CF \
         target (stock impl needs a working std::Path::is_absolute, \
         which wasm32-unknown-unknown lacks)."
    }

    fn is_const(&self) -> bool {
        true
    }

    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        Err(ShellError::GenericError {
            msg: "path self can only run at parse time".into(),
            error: "wrap in a `const` binding".into(),
            span: Some(call.head),
            help: Some("e.g. `const here = path self`".into()),
            inner: Vec::new(),
        })
    }

    fn run_const(
        &self,
        working_set: &StateWorkingSet,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;
        let extra: Option<String> = call.opt_const(working_set, 0)?;
        let result = match extra {
            Some(p) if p.starts_with('/') => p,
            Some(p) => format!("/{p}"),
            None => "/handler.nu".to_string(),
        };
        Ok(Value::string(result, span).into_pipeline_data())
    }
}
