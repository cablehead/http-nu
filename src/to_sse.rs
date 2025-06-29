use nu_engine::command_prelude::*;
use nu_protocol::{ByteStream, ByteStreamType, Config, PipelineMetadata, Span, Value};
use serde_json;

const LINE_ENDING: &str = "\n";

#[derive(Clone)]
pub struct ToSse;

impl Command for ToSse {
    fn name(&self) -> &str {
        "to sse"
    }

    fn signature(&self) -> Signature {
        Signature::build("to sse")
            .input_output_types(vec![(Type::record(), Type::String)])
            .category(Category::Formats)
    }

    fn description(&self) -> &str {
        "Convert records into text/event-stream format"
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["sse", "server", "event"]
    }

    fn examples(&self) -> Vec<Example> {
        vec![Example {
            description: "Convert a record into a server-sent event",
            example: "{data: 'hello'} | to sse",
            result: Some(Value::test_string("data: hello\n\n")),
        }]
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let config = stack.get_config(engine_state);
        match input {
            PipelineData::ListStream(stream, meta) => {
                let span = stream.span();
                let cfg = config.clone();
                let iter = stream
                    .into_iter()
                    .map(move |val| event_to_string(&cfg, val));
                let stream = ByteStream::from_result_iter(
                    iter,
                    span,
                    engine_state.signals().clone(),
                    ByteStreamType::String,
                );
                Ok(PipelineData::ByteStream(stream, update_metadata(meta)))
            }
            PipelineData::Value(Value::List { vals, .. }, meta) => {
                let cfg = config.clone();
                let iter = vals.into_iter().map(move |val| event_to_string(&cfg, val));
                let span = head;
                let stream = ByteStream::from_result_iter(
                    iter,
                    span,
                    engine_state.signals().clone(),
                    ByteStreamType::String,
                );
                Ok(PipelineData::ByteStream(stream, update_metadata(meta)))
            }
            PipelineData::Value(val, meta) => {
                let out = event_to_string(&config, val)?;
                Ok(
                    Value::string(out, head)
                        .into_pipeline_data_with_metadata(update_metadata(meta)),
                )
            }
            PipelineData::Empty => Ok(PipelineData::Value(
                Value::string(String::new(), head),
                update_metadata(None),
            )),
            PipelineData::ByteStream(..) => Err(ShellError::TypeMismatch {
                err_message: "expected record input".into(),
                span: head,
            }),
        }
    }
}

#[allow(clippy::result_large_err)]
fn event_to_string(config: &Config, val: Value) -> Result<String, ShellError> {
    let span = val.span();
    let rec = match val {
        Value::Record { val, .. } => val,
        other => {
            return Err(ShellError::TypeMismatch {
                err_message: format!("expected record, got {}", other.get_type()),
                span,
            })
        }
    };
    let mut out = String::new();
    if let Some(id) = rec.get("id") {
        out.push_str("id: ");
        out.push_str(&id.to_expanded_string("", config));
        out.push_str(LINE_ENDING);
    }
    if let Some(event) = rec.get("event") {
        out.push_str("event: ");
        out.push_str(&event.to_expanded_string("", config));
        out.push_str(LINE_ENDING);
    }
    if let Some(data) = rec.get("data") {
        let data_str = match data {
            Value::String { val, .. } => val.clone(),
            _ => {
                let json_value =
                    value_to_json(data, config).map_err(|err| ShellError::GenericError {
                        error: err.to_string(),
                        msg: "failed to serialize json".into(),
                        span: Some(Span::unknown()),
                        help: None,
                        inner: vec![],
                    })?;
                serde_json::to_string(&json_value).map_err(|err| ShellError::GenericError {
                    error: err.to_string(),
                    msg: "failed to serialize json".into(),
                    span: Some(Span::unknown()),
                    help: None,
                    inner: vec![],
                })?
            }
        };
        for line in data_str.lines() {
            out.push_str("data: ");
            out.push_str(line);
            out.push_str(LINE_ENDING);
        }
    }
    out.push_str(LINE_ENDING);
    Ok(out)
}

fn value_to_json(val: &Value, config: &Config) -> serde_json::Result<serde_json::Value> {
    Ok(match val {
        Value::Bool { val, .. } => serde_json::Value::Bool(*val),
        Value::Int { val, .. } => serde_json::Value::from(*val),
        Value::Float { val, .. } => serde_json::Number::from_f64(*val)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String { val, .. } => serde_json::Value::String(val.clone()),
        Value::List { vals, .. } => serde_json::Value::Array(
            vals.iter()
                .map(|v| value_to_json(v, config))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Value::Record { val, .. } => {
            let mut map = serde_json::Map::new();
            for (k, v) in val.iter() {
                map.insert(k.clone(), value_to_json(v, config)?);
            }
            serde_json::Value::Object(map)
        }
        Value::Nothing { .. } => serde_json::Value::Null,
        other => serde_json::Value::String(other.to_expanded_string("", config)),
    })
}

fn update_metadata(metadata: Option<PipelineMetadata>) -> Option<PipelineMetadata> {
    metadata
        .map(|md| md.with_content_type(Some("text/event-stream".into())))
        .or_else(|| {
            Some(PipelineMetadata::default().with_content_type(Some("text/event-stream".into())))
        })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_content_type_metadata() {
        use nu_cmd_lang::eval_pipeline_without_terminal_expression;
        use nu_command::{Get, Metadata};
        let mut engine_state = Box::new(EngineState::new());
        let delta = {
            let mut working_set = StateWorkingSet::new(&engine_state);
            working_set.add_decl(Box::new(ToSse {}));
            working_set.add_decl(Box::new(Metadata {}));
            working_set.add_decl(Box::new(Get {}));
            working_set.render()
        };
        engine_state.merge_delta(delta).expect("merge");
        let cmd = "{data: 'x'} | to sse | metadata | get content_type";
        let result = eval_pipeline_without_terminal_expression(
            cmd,
            std::env::temp_dir().as_ref(),
            &mut engine_state,
        );
        assert_eq!(
            Value::test_record(record!("content_type" => Value::test_string("text/event-stream"))),
            result.expect("result")
        );
    }

    #[test]
    fn test_full_event_output() {
        let record = record! {
            "id" => Value::test_string("42"),
            "event" => Value::test_string("greeting"),
            "data" => Value::test_string("Hello\nWorld"),
        };
        let val = Value::record(record, Span::unknown());
        let out = event_to_string(&Config::default(), val).unwrap();
        let expected = "id: 42\nevent: greeting\ndata: Hello\ndata: World\n\n";
        assert_eq!(out, expected);
    }

    #[test]
    fn test_non_record_error() {
        let err = event_to_string(&Config::default(), Value::test_int(123)).unwrap_err();
        match err {
            ShellError::TypeMismatch { err_message, span } => {
                assert_eq!(span, Span::test_data());
                assert!(err_message.contains("expected record"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
