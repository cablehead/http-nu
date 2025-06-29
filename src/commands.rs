use crate::response::{Response, ResponseBodyType};
use nu_engine::command_prelude::*;
use nu_protocol::{Category, PipelineData, ShellError, Signature, SyntaxShape, Type, Value};
use std::cell::RefCell;
use std::collections::HashMap;
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
                    map.insert(k.clone(), v.as_str()?.to_string());
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

        let response = Response {
            status: 200,
            headers: HashMap::new(),
            body_type: ResponseBodyType::Static {
                root: PathBuf::from(root),
                path,
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
