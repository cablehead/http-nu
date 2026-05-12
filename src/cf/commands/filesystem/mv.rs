//! `mv` shadow. Mirrors `nu-command/src/filesystem/mv.rs` (or `umv.rs`
//! depending on the Nu release).

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::commands::shared::{normalise_input, require_vfs, vfs_err};

#[derive(Clone, Default)]
pub struct VfsMv;

impl Command for VfsMv {
    fn name(&self) -> &str {
        "mv"
    }
    fn signature(&self) -> Signature {
        Signature::build("mv")
            .required("src", SyntaxShape::String, "source path")
            .required("dst", SyntaxShape::String, "destination path")
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::FileSystem)
    }
    fn description(&self) -> &str {
        "Rename or move a file via the active Vfs."
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
                .map_err(|e| vfs_err(span, format!("mv: {e}"), format!("read_bytes({src})")))?;
            v.write(Path::new(&dst), &bytes)
                .map_err(|e| vfs_err(span, format!("mv: {e}"), format!("write({dst})")))?;
            v.rm(Path::new(&src))
                .map_err(|e| vfs_err(span, format!("mv: {e}"), format!("rm({src})")))
        })?;
        Ok(PipelineData::Empty)
    }
}
