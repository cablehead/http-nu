//! `path exists` shadow. Mirrors `nu-command/src/path/exists.rs`.

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::commands::shared::normalise_input;
use crate::cf::vfs::with_vfs;

#[derive(Clone, Default)]
pub struct VfsPathExists;

impl Command for VfsPathExists {
    fn name(&self) -> &str {
        "path exists"
    }
    fn signature(&self) -> Signature {
        Signature::build("path exists")
            .optional(
                "path",
                SyntaxShape::String,
                "path to check (else from pipeline)",
            )
            .input_output_types(vec![
                (Type::String, Type::Bool),
                (Type::Nothing, Type::Bool),
            ])
            .category(Category::FileSystem)
    }
    fn description(&self) -> &str {
        "Whether a path exists in the active Vfs."
    }
    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;
        let path: Option<String> = call.opt(engine_state, stack, 0)?;
        let path = match path {
            Some(p) => p,
            None => match input.into_value(span)? {
                Value::String { val, .. } => val,
                other => other.coerce_into_string()?,
            },
        };
        let path = normalise_input(&path);
        let exists = with_vfs(|v| v.is_some_and(|v| v.exists(Path::new(&path))));
        Ok(Value::bool(exists, span).into_pipeline_data())
    }
}
