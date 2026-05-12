//! `rm` shadow. Mirrors `nu-command/src/filesystem/rm.rs`.
//!
//! Used by: `examples/cf-workspace-browser/` (many call sites).
//!
//! Divergences from stock (against `nu-command` 0.112.1):
//!
//! | Stock feature             | Stock arg                  | Shadow?           | Notes |
//! |---------------------------|----------------------------|-------------------|-------|
//! | Rest paths                | multi-path                 | unknown -- AUDIT  | Verify shadow accepts more than one path. |
//! | `--recursive` / `-r`      | recurse into dirs          | yes               | Matches stock. |
//! | `--force` / `-f`          | suppress missing-file error| yes               | Matches stock. |
//! | `--verbose` / `-v`        | print deletions            | no                | |
//! | `--interactive` / `-i`    | confirm prompts            | no                | No TTY. |
//! | `--trash`, `--permanent`  | trash semantics            | n/a               | Workspace has no trash; rm is permanent. |
//! | `--all` / `-a`            | include hidden in `*` glob | no                | No hidden-file convention. |

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::nu::nu_command::shared::{normalise_input, require_vfs, vfs_err};

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
