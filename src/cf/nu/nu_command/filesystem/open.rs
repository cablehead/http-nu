//! `open` shadow. Mirrors `nu-command/src/filesystem/open.rs`.
//!
//! Used by: `examples/cf-workspace-browser/`, `examples/tao/`,
//! `examples/cargo-docs/` (and any demo that does `open foo.json`).
//!
//! Divergences from stock (against `nu-command` 0.112.1):
//!
//! | Stock feature       | Stock arg                  | Shadow?         | Notes |
//! |---------------------|----------------------------|-----------------|-------|
//! | Rest filenames      | multi-file open            | no -- AUDIT     | Stock accepts `open a b c`; we accept one positional path. |
//! | `--raw` / `-r`      | bytes-only, skip mime decode | yes           | Returns Value::binary; same as stock. |
//! | Extension dispatch  | .json -> record, .yaml -> record, etc. | partial -- json only | Stock dispatches via MIME / from-cmd registry. We only auto-parse `.json` today; other formats (yaml/toml/csv) return a String -- callers should pipe through `\| from yaml` etc. explicitly. Adding more extensions = audit `nu-cmd-extra`'s `from <fmt>` registry. |

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
        "Read a file via the active Vfs. Auto-parses .json by extension; pass --raw for bytes."
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
        let s = String::from_utf8(bytes).map_err(|e| {
            vfs_err(
                span,
                "open: file is not valid UTF-8 (use --raw)",
                e.to_string(),
            )
        })?;
        let ext = Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("json") => Ok(parse_json(&s, span)?.into_pipeline_data()),
            _ => Ok(Value::string(s, span).into_pipeline_data()),
        }
    }
}

fn parse_json(s: &str, span: Span) -> Result<Value, ShellError> {
    let v: serde_json::Value = serde_json::from_str(s).map_err(|e| {
        vfs_err(
            span,
            "open: JSON parse error",
            format!("{e} (use --raw to bypass)"),
        )
    })?;
    Ok(json_to_value(v, span))
}

fn json_to_value(v: serde_json::Value, span: Span) -> Value {
    use serde_json::Value as J;
    match v {
        J::Null => Value::nothing(span),
        J::Bool(b) => Value::bool(b, span),
        J::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::int(i, span)
            } else if let Some(f) = n.as_f64() {
                Value::float(f, span)
            } else {
                Value::string(n.to_string(), span)
            }
        }
        J::String(s) => Value::string(s, span),
        J::Array(arr) => Value::list(
            arr.into_iter().map(|v| json_to_value(v, span)).collect(),
            span,
        ),
        J::Object(map) => {
            let mut rec = nu_protocol::Record::new();
            for (k, v) in map {
                rec.push(k, json_to_value(v, span));
            }
            Value::record(rec, span)
        }
    }
}
