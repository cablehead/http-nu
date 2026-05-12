//! `open` shadow. Mirrors `nu-command/src/filesystem/open.rs`.
//!
//! Used by: `examples/cf-workspace-browser/`.
//!
//! Divergences from stock (against `nu-command` 0.112.1):
//!
//! | Stock feature       | Stock arg                  | Shadow?           | Notes |
//! |---------------------|----------------------------|-------------------|-------|
//! | Rest filenames      | multi-file open            | unknown -- AUDIT  | Verify shadow accepts multiple positional paths. |
//! | `--raw` / `-r`      | bytes-only, skip mime decode | unknown -- AUDIT | If shadow always decodes by mime, `--raw` scripts get different output than desktop. |

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::nu::nu_command::shared::{normalise_input, require_vfs, vfs_err};

#[derive(Clone, Default)]
pub struct VfsOpen;

impl Command for VfsOpen {
    fn name(&self) -> &str {
        "open"
    }
    fn signature(&self) -> Signature {
        Signature::build("open")
            .required("path", SyntaxShape::String, "file path to open")
            .switch("raw", "return raw bytes instead of text", Some('r'))
            .input_output_types(vec![(Type::Nothing, Type::Any)])
            .category(Category::FileSystem)
    }
    fn description(&self) -> &str {
        "Read a file via the active Vfs."
    }
    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;
        let path: String = call.req(engine_state, stack, 0)?;
        let raw = call.has_flag(engine_state, stack, "raw")?;
        let path = normalise_input(&path);
        let bytes = require_vfs(span, |v| {
            v.read_bytes(Path::new(&path))
                .map_err(|e| vfs_err(span, format!("open: {e}"), format!("read_bytes({path})")))
        })?;
        if raw {
            return Ok(Value::binary(bytes, span).into_pipeline_data());
        }
        match String::from_utf8(bytes) {
            Ok(s) => Ok(Value::string(s, span).into_pipeline_data()),
            Err(e) => Err(vfs_err(
                span,
                "open: file is not valid UTF-8 (use --raw)",
                e.to_string(),
            )),
        }
    }
}
