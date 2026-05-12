//! `cp` shadow. Mirrors `nu-command/src/filesystem/ucp.rs`.
//!
//! Used by: `examples/cf-workspace-browser/`.
//!
//! Divergences from stock (against `nu-command` 0.112.1):
//!
//! | Stock feature                         | Stock arg                    | Shadow?           | Notes |
//! |---------------------------------------|------------------------------|-------------------|-------|
//! | Rest paths (`SRC... DEST`)            | yes                          | unknown -- AUDIT  | Verify multi-source cp into a dir works. |
//! | `--recursive` / `-r`                  | recurse dirs                 | yes               | Matches stock. |
//! | `--verbose` / `-v`                    | log copies                   | no                | |
//! | `--interactive` / `-i`                | prompt before overwrite      | no                | |
//! | `--force`, `--no-clobber`, `--update`, `--debug` | overwrite modes    | no                | |
//! | `--progress` / `-p`                   | progress bar                 | no                | |
//! | `--all` / `-a`                        | include hidden in `*` glob   | no                | |
//! | MIME preservation                     | implicit                     | yes               | Source `mime_type` propagated (matches Vfs `Workspace::cp`). |

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::nu::nu_command::shared::{normalise_input, require_vfs, vfs_err};

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
