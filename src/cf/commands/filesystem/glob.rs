//! `glob` shadow. Mirrors `nu-command/src/filesystem/glob.rs`.

use nu_engine::command_prelude::*;

use crate::cf::vfs::with_vfs;

#[derive(Clone, Default)]
pub struct VfsGlob;

impl Command for VfsGlob {
    fn name(&self) -> &str {
        "glob"
    }
    fn signature(&self) -> Signature {
        Signature::build("glob")
            .required("pattern", SyntaxShape::String, "glob pattern")
            .input_output_types(vec![(Type::Nothing, Type::List(Box::new(Type::String)))])
            .category(Category::FileSystem)
    }
    fn description(&self) -> &str {
        "Match paths in the active Vfs against a glob (`*`, `?`)."
    }
    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let span = call.head;
        let pattern: String = call.req(engine_state, stack, 0)?;
        let pattern = if pattern.starts_with('/') || pattern.contains('*') || pattern.contains('?')
        {
            pattern
        } else {
            format!("/{pattern}")
        };
        let mut matches: Vec<String> = Vec::new();
        with_vfs(|v| {
            if let Some(v) = v {
                v.for_each_path(&mut |p| {
                    if glob_match(&pattern, p) {
                        matches.push(p.to_string());
                    }
                });
            }
        });
        matches.sort();
        let values: Vec<Value> = matches
            .into_iter()
            .map(|p| Value::string(p, span))
            .collect();
        Ok(Value::list(values, span).into_pipeline_data())
    }
}

fn glob_match(pat: &str, s: &str) -> bool {
    let p: Vec<char> = pat.chars().collect();
    let s: Vec<char> = s.chars().collect();
    match_recursive(&p, 0, &s, 0)
}

fn match_recursive(p: &[char], pi: usize, s: &[char], si: usize) -> bool {
    if pi == p.len() {
        return si == s.len();
    }
    match p[pi] {
        '*' => (si..=s.len()).any(|i| match_recursive(p, pi + 1, s, i)),
        '?' => si < s.len() && match_recursive(p, pi + 1, s, si + 1),
        c => si < s.len() && s[si] == c && match_recursive(p, pi + 1, s, si + 1),
    }
}
