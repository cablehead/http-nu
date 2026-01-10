use crate::response::{Response, ResponseBodyType};
use nu_engine::command_prelude::*;
use nu_protocol::{
    ByteStream, ByteStreamType, Category, Config, CustomValue, PipelineData, PipelineMetadata,
    ShellError, Signature, Span, SyntaxShape, Type, Value,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use tokio::sync::oneshot;

use minijinja::{path_loader, Environment};
use std::sync::{Arc, OnceLock, RwLock};

use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

// === Template Cache ===

type TemplateCache = RwLock<HashMap<u128, Arc<Environment<'static>>>>;

static TEMPLATE_CACHE: OnceLock<TemplateCache> = OnceLock::new();

fn get_cache() -> &'static TemplateCache {
    TEMPLATE_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn hash_source(source: &str) -> u128 {
    xxhash_rust::xxh3::xxh3_128(source.as_bytes())
}

/// Compile template and insert into cache. Returns hash.
fn compile_template(source: &str) -> Result<u128, minijinja::Error> {
    let hash = hash_source(source);

    let mut cache = get_cache().write().unwrap();
    if cache.contains_key(&hash) {
        return Ok(hash);
    }

    let mut env = Environment::new();
    env.add_template_owned("template".to_string(), source.to_string())?;
    cache.insert(hash, Arc::new(env));
    Ok(hash)
}

/// Get compiled template from cache by hash.
fn get_compiled(hash: u128) -> Option<Arc<Environment<'static>>> {
    get_cache().read().unwrap().get(&hash).map(Arc::clone)
}

// === CompiledTemplate CustomValue ===

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompiledTemplate {
    hash: u128,
}

impl CompiledTemplate {
    /// Render this template with the given context
    pub fn render(&self, context: &minijinja::Value) -> Result<String, minijinja::Error> {
        let env = get_compiled(self.hash).expect("template not in cache");
        let tmpl = env.get_template("template")?;
        tmpl.render(context)
    }
}

#[typetag::serde]
impl CustomValue for CompiledTemplate {
    fn clone_value(&self, span: Span) -> Value {
        Value::custom(Box::new(self.clone()), span)
    }

    fn type_name(&self) -> String {
        "CompiledTemplate".into()
    }

    fn to_base_value(&self, span: Span) -> Result<Value, ShellError> {
        Ok(Value::string(format!("{:032x}", self.hash), span))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

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
            .input_output_types(vec![
                (Type::record(), Type::String),
                (Type::List(Box::new(Type::record())), Type::String),
            ])
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

fn emit_data_lines(out: &mut String, s: &str) {
    for line in s.lines() {
        out.push_str("data: ");
        out.push_str(line);
        out.push_str(LINE_ENDING);
    }
}

#[allow(clippy::result_large_err)]
fn value_to_data_string(val: &Value, config: &Config) -> Result<String, ShellError> {
    match val {
        Value::String { val, .. } => Ok(val.clone()),
        _ => {
            let json_value =
                value_to_json(val, config).map_err(|err| ShellError::GenericError {
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
            })
        }
    }
}

#[allow(clippy::result_large_err)]
fn event_to_string(config: &Config, val: Value) -> Result<String, ShellError> {
    let span = val.span();
    let rec = match val {
        Value::Record { val, .. } => val,
        // Propagate the original error instead of creating a new "expected record" error
        Value::Error { error, .. } => return Err(*error),
        other => {
            return Err(ShellError::TypeMismatch {
                err_message: format!("expected record, got {}", other.get_type()),
                span,
            })
        }
    };
    let mut out = String::new();
    if let Some(id) = rec.get("id") {
        if !matches!(id, Value::Nothing { .. }) {
            out.push_str("id: ");
            out.push_str(&id.to_expanded_string("", config));
            out.push_str(LINE_ENDING);
        }
    }
    if let Some(retry) = rec.get("retry") {
        if !matches!(retry, Value::Nothing { .. }) {
            out.push_str("retry: ");
            out.push_str(&retry.to_expanded_string("", config));
            out.push_str(LINE_ENDING);
        }
    }
    if let Some(event) = rec.get("event") {
        if !matches!(event, Value::Nothing { .. }) {
            out.push_str("event: ");
            out.push_str(&event.to_expanded_string("", config));
            out.push_str(LINE_ENDING);
        }
    }
    if let Some(data) = rec.get("data") {
        if !matches!(data, Value::Nothing { .. }) {
            match data {
                Value::List { vals, .. } => {
                    for item in vals {
                        emit_data_lines(&mut out, &value_to_data_string(item, config)?);
                    }
                }
                _ => {
                    emit_data_lines(&mut out, &value_to_data_string(data, config)?);
                }
            }
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

#[derive(Clone)]
pub struct MjCommand;

impl Default for MjCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl MjCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for MjCommand {
    fn name(&self) -> &str {
        ".mj"
    }

    fn description(&self) -> &str {
        "Render a minijinja template with context from input"
    }

    fn signature(&self) -> Signature {
        Signature::build(".mj")
            .optional("file", SyntaxShape::String, "template file path")
            .named(
                "inline",
                SyntaxShape::String,
                "inline template string",
                Some('i'),
            )
            .input_output_types(vec![(Type::Record(vec![].into()), Type::String)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let file: Option<String> = call.opt(engine_state, stack, 0)?;
        let inline: Option<String> = call.get_flag(engine_state, stack, "inline")?;

        // Validate arguments
        if file.is_some() && inline.is_some() {
            return Err(ShellError::GenericError {
                error: "Cannot specify both file and --inline".into(),
                msg: "use either a file path or --inline, not both".into(),
                span: Some(head),
                help: None,
                inner: vec![],
            });
        }
        if file.is_none() && inline.is_none() {
            return Err(ShellError::GenericError {
                error: "No template specified".into(),
                msg: "provide a file path or use --inline".into(),
                span: Some(head),
                help: None,
                inner: vec![],
            });
        }

        // Get context from input
        let context = match input {
            PipelineData::Value(val, _) => nu_value_to_minijinja(&val),
            PipelineData::Empty => minijinja::Value::from(()),
            _ => {
                return Err(ShellError::TypeMismatch {
                    err_message: "expected record input".into(),
                    span: head,
                });
            }
        };

        // Set up environment and get template
        let mut env = Environment::new();
        let tmpl = if let Some(ref path) = file {
            let path = std::path::Path::new(path);
            let abs_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir().unwrap_or_default().join(path)
            };
            if let Some(parent) = abs_path.parent() {
                env.set_loader(path_loader(parent));
            }
            let name = abs_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("template");
            env.get_template(name)
                .map_err(|e| ShellError::GenericError {
                    error: format!("Template error: {e}"),
                    msg: e.to_string(),
                    span: Some(head),
                    help: None,
                    inner: vec![],
                })?
        } else {
            let source = inline.as_ref().unwrap();
            env.add_template("template", source)
                .map_err(|e| ShellError::GenericError {
                    error: format!("Template parse error: {e}"),
                    msg: e.to_string(),
                    span: Some(head),
                    help: None,
                    inner: vec![],
                })?;
            env.get_template("template")
                .map_err(|e| ShellError::GenericError {
                    error: format!("Failed to get template: {e}"),
                    msg: e.to_string(),
                    span: Some(head),
                    help: None,
                    inner: vec![],
                })?
        };

        let rendered = tmpl
            .render(&context)
            .map_err(|e| ShellError::GenericError {
                error: format!("Template render error: {e}"),
                msg: e.to_string(),
                span: Some(head),
                help: None,
                inner: vec![],
            })?;

        Ok(Value::string(rendered, head).into_pipeline_data())
    }
}

/// Convert a nu_protocol::Value to a minijinja::Value via serde_json
fn nu_value_to_minijinja(val: &Value) -> minijinja::Value {
    let json = value_to_json(val, &Config::default()).unwrap_or(serde_json::Value::Null);
    minijinja::Value::from_serialize(&json)
}

// === .mj compile ===

#[derive(Clone)]
pub struct MjCompileCommand;

impl Default for MjCompileCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl MjCompileCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for MjCompileCommand {
    fn name(&self) -> &str {
        ".mj compile"
    }

    fn description(&self) -> &str {
        "Compile a minijinja template, returning a reusable compiled template"
    }

    fn signature(&self) -> Signature {
        Signature::build(".mj compile")
            .optional("file", SyntaxShape::String, "template file path")
            .named(
                "inline",
                SyntaxShape::Any,
                "inline template (string or {__html: string})",
                Some('i'),
            )
            .input_output_types(vec![(
                Type::Nothing,
                Type::Custom("CompiledTemplate".into()),
            )])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let file: Option<String> = call.opt(engine_state, stack, 0)?;
        let inline: Option<Value> = call.get_flag(engine_state, stack, "inline")?;

        // Extract template string from --inline value (string or {__html: string})
        let inline_str: Option<String> = match &inline {
            None => None,
            Some(val) => match val {
                Value::String { val, .. } => Some(val.clone()),
                Value::Record { val, .. } => {
                    if let Some(html_val) = val.get("__html") {
                        match html_val {
                            Value::String { val, .. } => Some(val.clone()),
                            _ => {
                                return Err(ShellError::GenericError {
                                    error: "__html must be a string".into(),
                                    msg: "expected string value".into(),
                                    span: Some(head),
                                    help: None,
                                    inner: vec![],
                                });
                            }
                        }
                    } else {
                        return Err(ShellError::GenericError {
                            error: "Record must have __html field".into(),
                            msg: "expected {__html: string}".into(),
                            span: Some(head),
                            help: None,
                            inner: vec![],
                        });
                    }
                }
                _ => {
                    return Err(ShellError::GenericError {
                        error: "--inline must be string or {__html: string}".into(),
                        msg: "invalid type".into(),
                        span: Some(head),
                        help: None,
                        inner: vec![],
                    });
                }
            },
        };

        // Get template source
        let template_source = match (&file, &inline_str) {
            (Some(_), Some(_)) => {
                return Err(ShellError::GenericError {
                    error: "Cannot specify both file and --inline".into(),
                    msg: "use either a file path or --inline, not both".into(),
                    span: Some(head),
                    help: None,
                    inner: vec![],
                });
            }
            (None, None) => {
                return Err(ShellError::GenericError {
                    error: "No template specified".into(),
                    msg: "provide a file path or use --inline".into(),
                    span: Some(head),
                    help: None,
                    inner: vec![],
                });
            }
            (Some(path), None) => {
                std::fs::read_to_string(path).map_err(|e| ShellError::GenericError {
                    error: format!("Failed to read template file: {e}"),
                    msg: "could not read file".into(),
                    span: Some(head),
                    help: None,
                    inner: vec![],
                })?
            }
            (None, Some(tmpl)) => tmpl.clone(),
        };

        // Compile and cache the template
        let hash = compile_template(&template_source).map_err(|e| ShellError::GenericError {
            error: format!("Template compile error: {e}"),
            msg: e.to_string(),
            span: Some(head),
            help: None,
            inner: vec![],
        })?;

        Ok(Value::custom(Box::new(CompiledTemplate { hash }), head).into_pipeline_data())
    }
}

// === .mj render ===

#[derive(Clone)]
pub struct MjRenderCommand;

impl Default for MjRenderCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl MjRenderCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for MjRenderCommand {
    fn name(&self) -> &str {
        ".mj render"
    }

    fn description(&self) -> &str {
        "Render a compiled minijinja template with context from input"
    }

    fn signature(&self) -> Signature {
        Signature::build(".mj render")
            .required(
                "template",
                SyntaxShape::Any,
                "compiled template from '.mj compile'",
            )
            .input_output_types(vec![(Type::Record(vec![].into()), Type::String)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let template_val: Value = call.req(engine_state, stack, 0)?;

        // Extract CompiledTemplate from the value
        let compiled = match template_val {
            Value::Custom { val, .. } => val
                .as_any()
                .downcast_ref::<CompiledTemplate>()
                .ok_or_else(|| ShellError::TypeMismatch {
                    err_message: "expected CompiledTemplate".into(),
                    span: head,
                })?
                .clone(),
            _ => {
                return Err(ShellError::TypeMismatch {
                    err_message: "expected CompiledTemplate from '.mj compile'".into(),
                    span: head,
                });
            }
        };

        // Get context from input
        let context = match input {
            PipelineData::Value(val, _) => nu_value_to_minijinja(&val),
            PipelineData::Empty => minijinja::Value::from(()),
            _ => {
                return Err(ShellError::TypeMismatch {
                    err_message: "expected record input".into(),
                    span: head,
                });
            }
        };

        // Render template
        let rendered = compiled
            .render(&context)
            .map_err(|e| ShellError::GenericError {
                error: format!("Template render error: {e}"),
                msg: e.to_string(),
                span: Some(head),
                help: None,
                inner: vec![],
            })?;

        Ok(Value::string(rendered, head).into_pipeline_data())
    }
}

// === Syntax Highlighting ===

struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
}

impl SyntaxHighlighter {
    fn new() -> Self {
        const SYNTAX_SET: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/syntax_set.bin"));
        let syntax_set = syntect::dumps::from_binary(SYNTAX_SET);
        Self { syntax_set }
    }

    fn highlight(&self, code: &str, lang: Option<&str>) -> String {
        let syntax = match lang {
            Some(lang) => self
                .syntax_set
                .find_syntax_by_token(lang)
                .or_else(|| self.syntax_set.find_syntax_by_extension(lang)),
            None => None,
        }
        .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut html_generator = ClassedHTMLGenerator::new_with_class_style(
            syntax,
            &self.syntax_set,
            ClassStyle::Spaced,
        );

        for line in LinesWithEndings::from(code) {
            let _ = html_generator.parse_html_for_line_which_includes_newline(line);
        }

        html_generator.finalize()
    }

    fn list_syntaxes(&self) -> Vec<(String, Vec<String>)> {
        self.syntax_set
            .syntaxes()
            .iter()
            .map(|s| (s.name.clone(), s.file_extensions.clone()))
            .collect()
    }
}

static HIGHLIGHTER: OnceLock<SyntaxHighlighter> = OnceLock::new();

fn get_highlighter() -> &'static SyntaxHighlighter {
    HIGHLIGHTER.get_or_init(SyntaxHighlighter::new)
}

// === .highlight command ===

#[derive(Clone)]
pub struct HighlightCommand;

impl Default for HighlightCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl HighlightCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for HighlightCommand {
    fn name(&self) -> &str {
        ".highlight"
    }

    fn description(&self) -> &str {
        "Syntax highlight code, outputting HTML with CSS classes"
    }

    fn signature(&self) -> Signature {
        Signature::build(".highlight")
            .required("lang", SyntaxShape::String, "language for highlighting")
            .input_output_types(vec![(Type::String, Type::record())])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let lang: String = call.req(engine_state, stack, 0)?;

        let code = match input {
            PipelineData::Value(Value::String { val, .. }, _) => val,
            PipelineData::ByteStream(stream, _) => stream.into_string()?,
            _ => {
                return Err(ShellError::TypeMismatch {
                    err_message: "expected string input".into(),
                    span: head,
                });
            }
        };

        let highlighter = get_highlighter();
        let html = highlighter.highlight(&code, Some(&lang));

        Ok(Value::record(
            nu_protocol::record! {
                "__html" => Value::string(html, head),
            },
            head,
        )
        .into_pipeline_data())
    }
}

// === .highlight theme command ===

#[derive(Clone)]
pub struct HighlightThemeCommand;

impl Default for HighlightThemeCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl HighlightThemeCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for HighlightThemeCommand {
    fn name(&self) -> &str {
        ".highlight theme"
    }

    fn description(&self) -> &str {
        "List available themes or get CSS for a specific theme"
    }

    fn signature(&self) -> Signature {
        Signature::build(".highlight theme")
            .optional("name", SyntaxShape::String, "theme name (omit to list all)")
            .input_output_types(vec![
                (Type::Nothing, Type::List(Box::new(Type::String))),
                (Type::Nothing, Type::String),
            ])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let name: Option<String> = call.opt(engine_state, stack, 0)?;

        let assets = syntect_assets::assets::HighlightingAssets::from_binary();

        match name {
            None => {
                let themes: Vec<Value> = assets.themes().map(|t| Value::string(t, head)).collect();
                Ok(Value::list(themes, head).into_pipeline_data())
            }
            Some(theme_name) => {
                let theme = assets.get_theme(&theme_name);
                let css = syntect::html::css_for_theme_with_class_style(theme, ClassStyle::Spaced)
                    .map_err(|e| ShellError::GenericError {
                        error: format!("Failed to generate CSS: {e}"),
                        msg: e.to_string(),
                        span: Some(head),
                        help: None,
                        inner: vec![],
                    })?;
                Ok(Value::string(css, head).into_pipeline_data())
            }
        }
    }
}

// === .highlight lang command ===

#[derive(Clone)]
pub struct HighlightLangCommand;

impl Default for HighlightLangCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl HighlightLangCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for HighlightLangCommand {
    fn name(&self) -> &str {
        ".highlight lang"
    }

    fn description(&self) -> &str {
        "List available languages for syntax highlighting"
    }

    fn signature(&self) -> Signature {
        Signature::build(".highlight lang")
            .input_output_types(vec![(Type::Nothing, Type::List(Box::new(Type::record())))])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let highlighter = get_highlighter();
        let langs: Vec<Value> = highlighter
            .list_syntaxes()
            .into_iter()
            .map(|(name, exts)| {
                Value::record(
                    nu_protocol::record! {
                        "name" => Value::string(name, head),
                        "extensions" => Value::list(
                            exts.into_iter().map(|e| Value::string(e, head)).collect(),
                            head
                        ),
                    },
                    head,
                )
            })
            .collect();
        Ok(Value::list(langs, head).into_pipeline_data())
    }
}

// === .md command ===

use pulldown_cmark::{html, CodeBlockKind, Event, Parser as MarkdownParser, Tag, TagEnd};

#[derive(Clone)]
pub struct MdCommand;

impl Default for MdCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl MdCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Command for MdCommand {
    fn name(&self) -> &str {
        ".md"
    }

    fn description(&self) -> &str {
        "Convert Markdown to HTML with syntax-highlighted code blocks"
    }

    fn signature(&self) -> Signature {
        Signature::build(".md")
            .input_output_types(vec![
                (Type::String, Type::record()),
                (Type::record(), Type::record()),
            ])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;

        // Determine if input is trusted ({__html: ...}) or untrusted (plain string)
        let (markdown, trusted) = match input.into_value(head)? {
            Value::String { val, .. } => (val, false),
            Value::Record { val, .. } => {
                if let Some(html_val) = val.get("__html") {
                    (html_val.as_str()?.to_string(), true)
                } else {
                    return Err(ShellError::TypeMismatch {
                        err_message: "expected string or {__html: ...}".into(),
                        span: head,
                    });
                }
            }
            other => {
                return Err(ShellError::TypeMismatch {
                    err_message: format!(
                        "expected string or {{__html: ...}}, got {}",
                        other.get_type()
                    ),
                    span: head,
                });
            }
        };

        let highlighter = get_highlighter();

        let mut in_code_block = false;
        let mut current_code = String::new();
        let mut current_lang: Option<String> = None;

        let parser = MarkdownParser::new(&markdown).map(|event| match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                current_code.clear();
                current_lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        let lang = info.split_whitespace().next().unwrap_or("");
                        if lang.is_empty() {
                            None
                        } else {
                            Some(lang.to_string())
                        }
                    }
                    CodeBlockKind::Indented => None,
                };
                Event::Text("".into())
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let highlighted = highlighter.highlight(&current_code, current_lang.as_deref());
                let mut html_out = String::new();
                html_out.push_str("<pre><code");
                if let Some(lang) = &current_lang {
                    html_out.push_str(&format!(" class=\"language-{lang}\""));
                }
                html_out.push('>');
                html_out.push_str(&highlighted);
                html_out.push_str("</code></pre>");
                Event::Html(html_out.into())
            }
            Event::Text(text) => {
                if in_code_block {
                    current_code.push_str(&text);
                    Event::Text("".into())
                } else {
                    Event::Text(text)
                }
            }
            // Escape raw HTML if input is untrusted
            Event::Html(html) => {
                if trusted {
                    Event::Html(html)
                } else {
                    Event::Text(html) // push_html escapes Text
                }
            }
            Event::InlineHtml(html) => {
                if trusted {
                    Event::InlineHtml(html)
                } else {
                    Event::Text(html)
                }
            }
            e => e,
        });

        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);

        Ok(Value::record(
            nu_protocol::record! {
                "__html" => Value::string(html_output, head),
            },
            head,
        )
        .into_pipeline_data())
    }
}
