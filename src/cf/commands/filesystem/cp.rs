//! `cp` shadow. Mirrors `nu-command/src/filesystem/ucp.rs`.

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::commands::shared::{normalise_input, require_vfs, vfs_err};

#[derive(Clone, Default)]
pub struct VfsCp;

impl Command for VfsCp {
    fn name(&self) -> &str {
        "cp"
    }
    fn signature(&self) -> Signature {
        Signature::build("cp")
            .required("src", SyntaxShape::String, "source path")
            .required("dst", SyntaxShape::String, "destination path")
            .switch("recursive", "recurse into directories", Some('r'))
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::FileSystem)
    }
    fn description(&self) -> &str {
        "Copy a file via the active Vfs."
    }
    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;
        let src: String = call.req(engine_state, stack, 0)?;
        let dst: String = call.req(engine_state, stack, 1)?;
        let src = normalise_input(&src);
        let dst = normalise_input(&dst);
        require_vfs(span, |v| {
            let bytes = v
                .read_bytes(Path::new(&src))
                .map_err(|e| vfs_err(span, format!("cp: {e}"), format!("read_bytes({src})")))?;
            v.write(Path::new(&dst), &bytes)
                .map_err(|e| vfs_err(span, format!("cp: {e}"), format!("write({dst})")))
        })?;
        Ok(PipelineData::Empty)
    }
}
