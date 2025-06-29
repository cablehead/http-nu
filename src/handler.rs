use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};

use serde::{Deserialize, Serialize};
use tower::util::ServiceExt;
use tower_http::services::ServeDir;

use tokio::sync::mpsc as tokio_mpsc;
use tokio::sync::oneshot;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::body::{Bytes, Frame};

use nu_engine::command_prelude::Type;
use nu_engine::CallExt;
use nu_protocol::engine::{Call, Command, EngineState, Job, Stack, ThreadJob};
use nu_protocol::{
    Category, PipelineData, Record, ShellError, Signature, Span, SyntaxShape, Value,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub proto: String,
    #[serde(with = "http_serde::method")]
    pub method: http::method::Method,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_ip: Option<std::net::IpAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
    #[serde(with = "http_serde::header_map")]
    pub headers: http::header::HeaderMap,
    #[serde(with = "http_serde::uri")]
    pub uri: http::Uri,
    pub path: String,
    pub query: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body_type: ResponseBodyType,
}

#[derive(Clone, Debug)]
pub enum ResponseBodyType {
    Normal,
    Static { root: PathBuf, path: String },
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type HTTPResult = Result<hyper::Response<BoxBody<Bytes, BoxError>>, BoxError>;

#[derive(Debug)]
enum ResponseTransport {
    Empty,
    Full(Vec<u8>),
    Stream(tokio_mpsc::Receiver<Vec<u8>>),
}

pub async fn handle(
    engine: Arc<crate::Engine>,
    addr: Option<SocketAddr>,
    req: hyper::Request<axum::body::Body>,
) -> Result<hyper::Response<BoxBody<Bytes, BoxError>>, std::convert::Infallible> {
    match handle_inner(engine, addr, req).await {
        Ok(response) => Ok(response),
        Err(err) => {
            eprintln!("Error handling request: {err}");
            let response = hyper::Response::builder()
                .status(500)
                .body(
                    Full::new("Internal Server Error".into())
                        .map_err(|never| match never {})
                        .boxed(),
                )
                .unwrap();
            Ok(response)
        }
    }
}

async fn handle_inner(
    engine: Arc<crate::Engine>,
    addr: Option<SocketAddr>,
    req: hyper::Request<axum::body::Body>,
) -> HTTPResult {
    let (parts, mut body) = req.into_parts();

    // Create channels for request body streaming
    let (body_tx, mut body_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, BoxError>>(32);

    // Spawn task to read request body frames
    tokio::task::spawn(async move {
        while let Some(frame) = body.frame().await {
            match frame {
                Ok(frame) => {
                    if let Some(data) = frame.data_ref() {
                        let bytes: Bytes = (*data).clone();
                        if body_tx.send(Ok(bytes.to_vec())).await.is_err() {
                            break;
                        }
                    }
                }
                Err(err) => {
                    let _ = body_tx.send(Err(err.into())).await;
                    break;
                }
            }
        }
    });

    // Create ByteStream for Nu pipeline
    let stream = nu_protocol::ByteStream::from_fn(
        nu_protocol::Span::unknown(),
        engine.state.signals().clone(),
        nu_protocol::ByteStreamType::Unknown,
        move |buffer: &mut Vec<u8>| match body_rx.blocking_recv() {
            Some(Ok(bytes)) => {
                buffer.extend_from_slice(&bytes);
                Ok(true)
            }
            Some(Err(err)) => Err(nu_protocol::ShellError::GenericError {
                error: "Body read error".into(),
                msg: err.to_string(),
                span: None,
                help: None,
                inner: vec![],
            }),
            None => Ok(false),
        },
    );

    let request = Request {
        proto: format!("{:?}", parts.version),
        method: parts.method.clone(),
        authority: parts.uri.authority().map(|a| a.to_string()),
        remote_ip: addr.as_ref().map(|a| a.ip()),
        remote_port: addr.as_ref().map(|a| a.port()),
        headers: parts.headers.clone(),
        uri: parts.uri.clone(),
        path: parts.uri.path().to_string(),
        query: parts
            .uri
            .query()
            .map(|v| {
                url::form_urlencoded::parse(v.as_bytes())
                    .into_owned()
                    .collect()
            })
            .unwrap_or_else(HashMap::new),
    };

    println!(
        "{}",
        serde_json::json!({"stamp": scru128::new(), "message": "request", "meta": request})
    );

    let (meta_rx, bridged_body) = spawn_eval_thread(engine, request, stream);

    // Wait for both:
    // 1. Metadata - either from .response or default values when closure skips .response
    // 2. Body pipeline to start (but not necessarily complete as it may stream)
    let (meta, body_result) = tokio::join!(
        async {
            meta_rx.await.unwrap_or(Response {
                status: 200,
                headers: HashMap::new(),
                body_type: ResponseBodyType::Normal,
            })
        },
        bridged_body
    );

    match &meta.body_type {
        ResponseBodyType::Normal => build_normal_response(&meta, Ok(body_result?)).await,
        ResponseBodyType::Static { root, path } => {
            let mut static_req = hyper::Request::new(axum::body::Body::empty());
            *static_req.uri_mut() = format!("/{}", path).parse().unwrap();
            *static_req.method_mut() = parts.method.clone();
            *static_req.headers_mut() = parts.headers.clone();

            let service = ServeDir::new(root);
            let res = service.oneshot(static_req).await.unwrap();
            let (parts, body) = res.into_parts();
            let bytes = body.collect().await.unwrap().to_bytes();
            let res = hyper::Response::from_parts(
                parts,
                Full::new(bytes).map_err(|e| match e {}).boxed(),
            );
            Ok(res)
        }
    }
}

async fn build_normal_response(
    meta: &Response,
    body_result: Result<(Option<String>, ResponseTransport), BoxError>,
) -> HTTPResult {
    let (inferred_content_type, body) = body_result?;
    let mut builder = hyper::Response::builder().status(meta.status);
    let mut header_map = hyper::header::HeaderMap::new();

    let content_type = meta
        .headers
        .get("content-type")
        .or(meta.headers.get("Content-Type"))
        .cloned()
        .or(inferred_content_type)
        .unwrap_or("text/html; charset=utf-8".to_string());

    header_map.insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_str(&content_type)?,
    );

    for (k, v) in &meta.headers {
        if k.to_lowercase() != "content-type" {
            header_map.insert(
                hyper::header::HeaderName::from_bytes(k.as_bytes())?,
                hyper::header::HeaderValue::from_str(v)?,
            );
        }
    }

    *builder.headers_mut().unwrap() = header_map;

    let body = match body {
        ResponseTransport::Empty => Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed(),
        ResponseTransport::Full(bytes) => Full::new(bytes.into())
            .map_err(|never| match never {})
            .boxed(),
        ResponseTransport::Stream(rx) => {
            let stream = ReceiverStream::new(rx).map(|data| Ok(Frame::data(Bytes::from(data))));
            StreamBody::new(stream).boxed()
        }
    };

    Ok(builder.body(body)?)
}

fn spawn_eval_thread(
    engine: Arc<crate::Engine>,
    request: Request,
    stream: nu_protocol::ByteStream,
) -> (
    oneshot::Receiver<Response>,
    oneshot::Receiver<(Option<String>, ResponseTransport)>,
) {
    let (meta_tx, meta_rx) = tokio::sync::oneshot::channel();
    let (body_tx, body_rx) = tokio::sync::oneshot::channel();

    fn inner(
        engine: Arc<crate::Engine>,
        request: Request,
        stream: nu_protocol::ByteStream,
        meta_tx: oneshot::Sender<Response>,
        body_tx: oneshot::Sender<(Option<String>, ResponseTransport)>,
    ) -> Result<(), BoxError> {
        RESPONSE_TX.with(|tx| {
            *tx.borrow_mut() = Some(meta_tx);
        });
        let result = engine.eval(request_to_value(&request, Span::unknown()), stream.into());
        // Always clear the thread local storage after eval completes
        RESPONSE_TX.with(|tx| {
            let _ = tx.borrow_mut().take(); // This will drop the sender if it wasn't used
        });
        let output = result?;
        let inferred_content_type = match &output {
            PipelineData::Value(Value::Record { .. }, meta)
                if meta.as_ref().and_then(|m| m.content_type.clone()).is_none() =>
            {
                Some("application/json".to_string())
            }
            PipelineData::Value(_, meta) | PipelineData::ListStream(_, meta) => {
                meta.as_ref().and_then(|m| m.content_type.clone())
            }
            _ => None,
        };
        match output {
            PipelineData::Empty => {
                let _ = body_tx.send((inferred_content_type, ResponseTransport::Empty));
                Ok(())
            }
            PipelineData::Value(Value::Nothing { .. }, _) => {
                let _ = body_tx.send((inferred_content_type, ResponseTransport::Empty));
                Ok(())
            }
            PipelineData::Value(value, _) => {
                let _ = body_tx.send((
                    inferred_content_type,
                    ResponseTransport::Full(value_to_bytes(value)),
                ));
                Ok(())
            }
            PipelineData::ListStream(stream, _) => {
                let (stream_tx, stream_rx) = tokio_mpsc::channel(32);
                let _ = body_tx.send((inferred_content_type, ResponseTransport::Stream(stream_rx)));
                for value in stream.into_inner() {
                    if stream_tx.blocking_send(value_to_bytes(value)).is_err() {
                        break;
                    }
                }
                Ok(())
            }
            PipelineData::ByteStream(stream, meta) => {
                let (stream_tx, stream_rx) = tokio_mpsc::channel(32);
                let _ = body_tx.send((
                    meta.as_ref().and_then(|m| m.content_type.clone()),
                    ResponseTransport::Stream(stream_rx),
                ));
                let mut reader = stream
                    .reader()
                    .ok_or_else(|| "ByteStream has no reader".to_string())?;
                let mut buf = vec![0; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            if stream_tx.blocking_send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(err) => return Err(err.into()),
                    }
                }
                Ok(())
            }
        }
    }

    // Create a thread job for this evaluation
    let (sender, _receiver) = mpsc::channel();
    let signals = engine.state.signals().clone();
    let job = ThreadJob::new(signals, Some("HTTP Request".to_string()), sender);

    // Add the job to the engine's job table
    let job_id = {
        let mut jobs = engine.state.jobs.lock().expect("jobs mutex poisoned");
        jobs.add_job(Job::Thread(job.clone()))
    };

    std::thread::spawn(move || -> Result<(), std::convert::Infallible> {
        // Create a local engine state with the job context
        let mut local_engine_state = engine.state.clone();
        local_engine_state.current_job.background_thread_job = Some(job);

        // Create a local engine with the job-aware state
        let local_engine = crate::Engine {
            state: local_engine_state.clone(),
            closure: engine.closure.clone(),
        };

        if let Err(e) = inner(Arc::new(local_engine), request, stream, meta_tx, body_tx) {
            eprintln!("Error in eval thread: {e}");
        }

        // Clean up job when done
        {
            let mut jobs = local_engine_state.jobs.lock().expect("jobs mutex poisoned");
            jobs.remove_job(job_id);
        }

        Ok(())
    });

    (meta_rx, body_rx)
}

fn value_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::Nothing { .. } => serde_json::Value::Null,
        Value::Bool { val, .. } => serde_json::Value::Bool(*val),
        Value::Int { val, .. } => serde_json::Value::Number((*val).into()),
        Value::Float { val, .. } => serde_json::Number::from_f64(*val)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::String { val, .. } => serde_json::Value::String(val.clone()),
        Value::List { vals, .. } => {
            serde_json::Value::Array(vals.iter().map(value_to_json).collect())
        }
        Value::Record { val, .. } => {
            let mut map = serde_json::Map::new();
            for (k, v) in val.iter() {
                map.insert(k.clone(), value_to_json(v));
            }
            serde_json::Value::Object(map)
        }
        _ => todo!(),
    }
}

fn value_to_bytes(value: Value) -> Vec<u8> {
    match value {
        Value::Nothing { .. } => Vec::new(),
        Value::String { val, .. } => val.into_bytes(),
        Value::Int { val, .. } => val.to_string().into_bytes(),
        Value::Float { val, .. } => val.to_string().into_bytes(),
        Value::Binary { val, .. } => val,
        Value::Bool { val, .. } => val.to_string().into_bytes(),

        // Both Lists and Records are encoded as JSON
        Value::List { .. } | Value::Record { .. } => serde_json::to_string(&value_to_json(&value))
            .unwrap_or_else(|_| String::new())
            .into_bytes(),

        _ => todo!("value_to_bytes: {:?}", value),
    }
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

thread_local! {
    static RESPONSE_TX: RefCell<Option<oneshot::Sender<Response>>> = const { RefCell::new(None) };
}

pub fn request_to_value(request: &Request, span: Span) -> Value {
    let mut record = Record::new();

    record.push("proto", Value::string(request.proto.clone(), span));
    record.push("method", Value::string(request.method.to_string(), span));
    record.push("uri", Value::string(request.uri.to_string(), span));
    record.push("path", Value::string(request.path.clone(), span));

    if let Some(authority) = &request.authority {
        record.push("authority", Value::string(authority.clone(), span));
    }

    if let Some(remote_ip) = &request.remote_ip {
        record.push("remote_ip", Value::string(remote_ip.to_string(), span));
    }

    if let Some(remote_port) = &request.remote_port {
        record.push("remote_port", Value::int(*remote_port as i64, span));
    }

    // Convert headers to a record
    let mut headers_record = Record::new();
    for (key, value) in request.headers.iter() {
        headers_record.push(
            key.to_string(),
            Value::string(value.to_str().unwrap_or_default().to_string(), span),
        );
    }
    record.push("headers", Value::record(headers_record, span));

    // Convert query parameters to a record
    let mut query_record = Record::new();
    for (key, value) in &request.query {
        query_record.push(key.clone(), Value::string(value.clone(), span));
    }
    record.push("query", Value::record(query_record, span));

    Value::record(record, span)
}
