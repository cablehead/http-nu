//! `mkdir` shadow. Mirrors `nu-command/src/filesystem/umkdir.rs`
//! (Nu calls its impl `umkdir` internally; the public name is `mkdir`).
//!
//! Used by: `examples/cf-workspace-browser/`.
//!
//! Divergences from stock (against `nu-command` 0.112.1):
//!
//! | Stock feature      | Stock arg              | Shadow?           | Notes |
//! |--------------------|------------------------|-------------------|-------|
//! | Rest paths         | multi-path             | yes               | First arg + rest both supported. |
//! | `--verbose` / `-v` | print created paths    | parsed but ignored | Switch is in signature; nothing prints. Either implement or remove. |
//! | Recursive          | implicit in stock      | yes               | Verify behaviour when intermediates exist. |

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::nu::nu_command::shared::{normalise_input, require_vfs, vfs_err};

#[derive(Clone, Default)]
pub struct VfsMkdir;

impl Command for VfsMkdir {
    fn name(&self) -> &str {
        "mkdir"
    }
    fn signature(&self) -> Signature {
        Signature::build("mkdir")
            .required("path", SyntaxShape::String, "directory path to create")
            .rest("rest", SyntaxShape::String, "additional paths")
            .switch("verbose", "print created paths", Some('v'))
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::FileSystem)
    }
    fn description(&self) -> &str {
        "Create directories in the active Vfs (recursive)."
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
                v.mkdir(Path::new(&p))
                    .map_err(|e| vfs_err(span, format!("mkdir: {e}"), format!("mkdir({p})")))?;
            }
            Ok(())
        })?;
        Ok(PipelineData::Empty)
    }
}
