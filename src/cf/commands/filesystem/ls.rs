//! `ls` shadow. Mirrors `nu-command/src/filesystem/ls.rs`.

use std::path::Path;

use nu_engine::command_prelude::*;

use crate::cf::commands::shared::{normalise_input, require_vfs, vfs_err};
use crate::cf::vfs::StatKind;

#[derive(Clone, Default)]
pub struct VfsLs;

impl Command for VfsLs {
    fn name(&self) -> &str {
        "ls"
    }
    fn signature(&self) -> Signature {
        Signature::build("ls")
            .optional(
                "path",
                SyntaxShape::String,
                "directory path to list (default '/')",
            )
            .input_output_types(vec![(Type::Nothing, Type::table())])
            .category(Category::FileSystem)
    }
    fn description(&self) -> &str {
        "List entries in a directory via the active Vfs."
    }
    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;
        let path: Option<String> = call.opt(engine_state, stack, 0)?;
        let path = normalise_input(&path.unwrap_or_else(|| "/".to_string()));
        let rows = require_vfs(span, |v| {
            let entries = v
                .read_dir(Path::new(&path))
                .map_err(|e| vfs_err(span, format!("ls: {e}"), format!("read_dir({path})")))?;
            let mut rows = Vec::with_capacity(entries.len());
            for entry in entries {
                let name = entry
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| entry.to_string_lossy().into_owned());
                let stat = v.stat(&entry).ok();
                let mut record = Record::new();
                record.insert("name", Value::string(name, span));
                match stat {
                    Some(stat) => {
                        record.insert(
                            "type",
                            Value::string(
                                match stat.kind {
                                    StatKind::File => "file",
                                    StatKind::Dir => "dir",
                                    StatKind::Symlink => "symlink",
                                },
                                span,
                            ),
                        );
                        record.insert("size", Value::filesize(stat.size as i64, span));
                    }
                    None => {
                        record.insert("type", Value::string("?", span));
                        record.insert("size", Value::filesize(0, span));
                    }
                }
                rows.push(Value::record(record, span));
            }
            Ok(rows)
        })?;
        Ok(Value::list(rows, span).into_pipeline_data())
    }
}
