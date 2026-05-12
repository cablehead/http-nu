//! `rm` shadow. Mirrors `nu-command/src/filesystem/rm.rs`.

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::commands::shared::{normalise_input, require_vfs, vfs_err};

#[derive(Clone, Default)]
pub struct VfsRm;

impl Command for VfsRm {
    fn name(&self) -> &str {
        "rm"
    }
    fn signature(&self) -> Signature {
        Signature::build("rm")
            .required("path", SyntaxShape::String, "path to remove")
            .rest("rest", SyntaxShape::String, "additional paths")
            .switch("recursive", "recurse into directories", Some('r'))
            .switch("force", "suppress errors on missing paths", Some('f'))
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::FileSystem)
    }
    fn description(&self) -> &str {
        "Remove a path from the active Vfs."
    }
    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;
        let first: String = call.req(engine_state, stack, 0)?;
        let rest: Vec<String> = call.rest(engine_state, stack, 1)?;
        require_vfs(span, |v| {
            for p in std::iter::once(first).chain(rest) {
                let p = normalise_input(&p);
                v.rm(Path::new(&p))
                    .map_err(|e| vfs_err(span, format!("rm: {e}"), format!("rm({p})")))?;
            }
            Ok(())
        })?;
        Ok(PipelineData::Empty)
    }
}
