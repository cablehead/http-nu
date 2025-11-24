use crate::response::{Response, ResponseBodyType};
use nu_engine::command_prelude::*;
use nu_protocol::{
    ByteStream, ByteStreamType, Category, Config, PipelineData, PipelineMetadata, ShellError,
    Signature, Span, SyntaxShape, Type, Value,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use tokio::sync::oneshot;

thread_local! {
    pub static RESPONSE_TX: RefCell<Option<oneshot::Sender<Response>>> = const { RefCell::new(None) };
}

#[derive(Clone)]
pub struct ResponseStartCommand;

impl Default for ResponseStartCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ResponseStartCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for ResponseStartCommand {
    fn name(&self) -> &str {
        ".response"
    }

    fn description(&self) -> &str {
        "Start an HTTP response with status and headers"
    }

    fn signature(&self) -> Signature {
        Signature::build(".response")
            .required(
                "meta",
                SyntaxShape::Record(vec![]), // Add empty vec argument
                "response configuration with optional status and headers",
            )
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let meta: Value = call.req(engine_state, stack, 0)?;
        let record = meta.as_record()?;

        // Extract optional status, default to 200
        let status = match record.get("status") {
            Some(status_value) => status_value.as_int()? as u16,
            None => 200,
        };

        // Extract headers
        let headers = match record.get("headers") {
            Some(headers_value) => {
                let headers_record = headers_value.as_record()?;
                let mut map = HashMap::new();
                for (k, v) in headers_record.iter() {
                    let header_value = match v {
                        Value::String { val, .. } => {
                            crate::response::HeaderValue::Single(val.clone())
                        }
                        Value::List { vals, .. } => {
                            let strings: Vec<String> = vals
                                .iter()
                                .filter_map(|v| v.as_str().ok())
                                .map(|s| s.to_string())
                                .collect();
                            crate::response::HeaderValue::Multiple(strings)
                        }
                        _ => {
                            return Err(nu_protocol::ShellError::CantConvert {
                                to_type: "string or list<string>".to_string(),
                                from_type: v.get_type().to_string(),
                                span: v.span(),
                                help: Some(
                                    "header values must be strings or lists of strings".to_string(),
                                ),
                            });
                        }
                    };
                    map.insert(k.clone(), header_value);
                }
                map
            }
            None => HashMap::new(),
        };

        // Create response and send through channel
        let response = Response {
            status,
            headers,
            body_type: ResponseBodyType::Normal,
        };

        RESPONSE_TX.with(|tx| -> Result<_, ShellError> {
            if let Some(tx) = tx.borrow_mut().take() {
                tx.send(response).map_err(|_| ShellError::GenericError {
                    error: "Failed to send response".into(),
                    msg: "Channel closed".into(),
                    span: Some(call.head),
                    help: None,
                    inner: vec![],
                })?;
            }
            Ok(())
        })?;

        Ok(PipelineData::Empty)
    }
}

#[derive(Clone)]
pub struct StaticCommand;

impl Default for StaticCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for StaticCommand {
    fn name(&self) -> &str {
        ".static"
    }

    fn description(&self) -> &str {
        "Serve static files from a directory"
    }

    fn signature(&self) -> Signature {
        Signature::build(".static")
            .required("root", SyntaxShape::String, "root directory path")
            .required("path", SyntaxShape::String, "request path")
            .named(
                "fallback",
                SyntaxShape::String,
                "fallback file when request missing",
                None,
            )
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let root: String = call.req(engine_state, stack, 0)?;
        let path: String = call.req(engine_state, stack, 1)?;

        let fallback: Option<String> = call.get_flag(engine_state, stack, "fallback")?;

        let response = Response {
            status: 200,
            headers: HashMap::new(),
            body_type: ResponseBodyType::Static {
                root: PathBuf::from(root),
                path,
                fallback,
            },
        };

        RESPONSE_TX.with(|tx| -> Result<_, ShellError> {
            if let Some(tx) = tx.borrow_mut().take() {
                tx.send(response).map_err(|_| ShellError::GenericError {
                    error: "Failed to send response".into(),
                    msg: "Channel closed".into(),
                    span: Some(call.head),
                    help: None,
                    inner: vec![],
                })?;
            }
            Ok(())
        })?;

        Ok(PipelineData::Empty)
    }
}

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

    fn examples(&self) -> Vec<Example<'_>> {
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

#[derive(Clone)]
pub struct ReverseProxyCommand;

impl Default for ReverseProxyCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl ReverseProxyCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for ReverseProxyCommand {
    fn name(&self) -> &str {
        ".reverse-proxy"
    }

    fn description(&self) -> &str {
        "Forward HTTP requests to a backend server"
    }

    fn signature(&self) -> Signature {
        Signature::build(".reverse-proxy")
            .required("target_url", SyntaxShape::String, "backend URL to proxy to")
            .optional(
                "config",
                SyntaxShape::Record(vec![]),
                "optional configuration (headers, preserve_host, strip_prefix, query)",
            )
            .input_output_types(vec![(Type::Any, Type::Nothing)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let target_url: String = call.req(engine_state, stack, 0)?;

        // Convert input pipeline data to bytes for request body
        let request_body = match input {
            PipelineData::Empty => Vec::new(),
            PipelineData::Value(value, _) => crate::response::value_to_bytes(value),
            PipelineData::ByteStream(stream, _) => {
                // Collect all bytes from the stream
                let mut body_bytes = Vec::new();
                if let Some(mut reader) = stream.reader() {
                    loop {
                        let mut buffer = vec![0; 8192];
                        match reader.read(&mut buffer) {
                            Ok(0) => break, // EOF
                            Ok(n) => {
                                buffer.truncate(n);
                                body_bytes.extend_from_slice(&buffer);
                            }
                            Err(_) => break,
                        }
                    }
                }
                body_bytes
            }
            PipelineData::ListStream(stream, _) => {
                // Convert list stream to JSON array
                let items: Vec<_> = stream.into_iter().collect();
                let json_value = serde_json::Value::Array(
                    items
                        .into_iter()
                        .map(|v| crate::response::value_to_json(&v))
                        .collect(),
                );
                serde_json::to_string(&json_value)
                    .unwrap_or_default()
                    .into_bytes()
            }
        };

        // Parse optional config
        let config = call.opt::<Value>(engine_state, stack, 1);

        let mut headers = HashMap::new();
        let mut preserve_host = true;
        let mut strip_prefix: Option<String> = None;
        let mut query: Option<HashMap<String, String>> = None;

        if let Ok(Some(config_value)) = config {
            if let Ok(record) = config_value.as_record() {
                // Extract headers
                if let Some(headers_value) = record.get("headers") {
                    if let Ok(headers_record) = headers_value.as_record() {
                        for (k, v) in headers_record.iter() {
                            let header_value = match v {
                                Value::String { val, .. } => {
                                    crate::response::HeaderValue::Single(val.clone())
                                }
                                Value::List { vals, .. } => {
                                    let strings: Vec<String> = vals
                                        .iter()
                                        .filter_map(|v| v.as_str().ok())
                                        .map(|s| s.to_string())
                                        .collect();
                                    crate::response::HeaderValue::Multiple(strings)
                                }
                                _ => continue, // Skip non-string/non-list values
                            };
                            headers.insert(k.clone(), header_value);
                        }
                    }
                }

                // Extract preserve_host
                if let Some(preserve_host_value) = record.get("preserve_host") {
                    if let Ok(ph) = preserve_host_value.as_bool() {
                        preserve_host = ph;
                    }
                }

                // Extract strip_prefix
                if let Some(strip_prefix_value) = record.get("strip_prefix") {
                    if let Ok(prefix) = strip_prefix_value.as_str() {
                        strip_prefix = Some(prefix.to_string());
                    }
                }

                // Extract query
                if let Some(query_value) = record.get("query") {
                    if let Ok(query_record) = query_value.as_record() {
                        let mut query_map = HashMap::new();
                        for (k, v) in query_record.iter() {
                            if let Ok(v_str) = v.as_str() {
                                query_map.insert(k.clone(), v_str.to_string());
                            }
                        }
                        query = Some(query_map);
                    }
                }
            }
        }

        let response = Response {
            status: 200,
            headers: HashMap::new(),
            body_type: ResponseBodyType::ReverseProxy {
                target_url,
                headers,
                preserve_host,
                strip_prefix,
                request_body,
                query,
            },
        };

        RESPONSE_TX.with(|tx| -> Result<_, ShellError> {
            if let Some(tx) = tx.borrow_mut().take() {
                tx.send(response).map_err(|_| ShellError::GenericError {
                    error: "Failed to send response".into(),
                    msg: "Channel closed".into(),
                    span: Some(call.head),
                    help: None,
                    inner: vec![],
                })?;
            }
            Ok(())
        })?;

        Ok(PipelineData::Empty)
    }
}
