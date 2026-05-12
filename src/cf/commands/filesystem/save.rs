//! `save` shadow. Mirrors `nu-command/src/filesystem/save.rs`.

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::commands::shared::{normalise_input, require_vfs, vfs_err};

#[derive(Clone, Default)]
pub struct VfsSave;

impl Command for VfsSave {
    fn name(&self) -> &str {
        "save"
    }
    fn signature(&self) -> Signature {
        Signature::build("save")
            .required("path", SyntaxShape::String, "file path to write")
            .switch("force", "overwrite without checking", Some('f'))
            .input_output_types(vec![(Type::Any, Type::Nothing)])
            .category(Category::FileSystem)
    }
    fn description(&self) -> &str {
        "Write pipeline input to a file via the active Vfs."
    }
    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;
        let path: String = call.req(engine_state, stack, 0)?;
        let path = normalise_input(&path);
        let bytes = match input.into_value(span)? {
            Value::String { val, .. } => val.into_bytes(),
            Value::Binary { val, .. } => val,
            other => other.coerce_into_string()?.into_bytes(),
        };
        require_vfs(span, |v| {
            v.write(Path::new(&path), &bytes)
                .map_err(|e| vfs_err(span, format!("save: {e}"), format!("write({path})")))
        })?;
        Ok(PipelineData::Empty)
    }
}
