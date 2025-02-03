use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;

use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::body::{Bytes, Frame};
use hyper::{Request, Response};

use nu_engine::command_prelude::Type;
use nu_engine::CallExt;
use nu_protocol::engine::{Call, Command, EngineState, Stack};
use nu_protocol::{Category, PipelineData, ShellError, Signature, SyntaxShape, Value};

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type HTTPResult = Result<Response<BoxBody<Bytes, BoxError>>, BoxError>;

enum ResponseTransport {
    Empty,
    Full(Vec<u8>),
    Stream(mpsc::Receiver<Vec<u8>>),
}

pub async fn handle<B>(
    engine: crate::Engine,
    script: String,
    addr: Option<SocketAddr>,
    req: Request<B>,
) -> Result<Response<BoxBody<Bytes, BoxError>>, BoxError>
where
    B: hyper::body::Body + Unpin + Send + 'static,
    B::Data: Into<Bytes> + Clone + Send,
    B::Error: Into<BoxError> + Send,
{
    match handle_inner(engine, script, addr, req).await {
        Ok(response) => Ok(response),
        Err(err) => {
            eprintln!("Handler error: {:?}", err);
            let response = Response::builder().status(500).body(
                Full::new("Internal Server Error".into())
                    .map_err(|never| match never {})
                    .boxed(),
            )?;
            Ok(response)
        }
    }
}

async fn handle_inner<B>(
    mut engine: crate::Engine,
    script: String,
    addr: Option<SocketAddr>,
    req: Request<B>,
) -> HTTPResult
where
    B: hyper::body::Body + Unpin + Send + 'static,
    B::Data: Into<Bytes> + Clone + Send,
    B::Error: Into<BoxError> + Send,
{
    // Create channels for response metadata
    let (meta_tx, meta_rx) = tokio::sync::oneshot::channel();

    // Add .response command to engine
    engine.add_commands(vec![Box::new(ResponseStartCommand::new(meta_tx))])?;
    engine.parse_closure(&script)?;

    let (parts, mut body) = req.into_parts();

    // Create channels for request body streaming
    let (body_tx, mut body_rx) = tokio::sync::mpsc::channel::<Result<Vec<u8>, BoxError>>(32);

    // Spawn task to read request body frames
    tokio::task::spawn(async move {
        while let Some(frame) = body.frame().await {
            match frame {
                Ok(frame) => {
                    if let Some(data) = frame.data_ref() {
                        let bytes: Bytes = (*data).clone().into();
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

    let request = crate::Request {
        proto: format!("{:?}", parts.version),
        method: parts.method,
        authority: parts.uri.authority().map(|a| a.to_string()),
        remote_ip: addr.as_ref().map(|a| a.ip()),
        remote_port: addr.as_ref().map(|a| a.port()),
        headers: parts.headers,
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

    let bridged_body = {
        let (body_tx, body_rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || -> Result<(), BoxError> {
            let output = engine.eval(request, stream.into())?;

            let inferred_content_type = match &output {
                PipelineData::Value(Value::Record { .. }, meta)
                    if meta.as_ref().and_then(|m| m.content_type).is_none() =>
                {
                    Some("application/json".to_string())
                }
                PipelineData::Value(_, meta) | PipelineData::ListStream(_, meta) => {
                    meta.and_then(|m| m.content_type)
                }
                _ => None,
            };

            let response = match output {
                PipelineData::Value(Value::Nothing { .. }, _) => ResponseTransport::Empty,
                PipelineData::Value(value, _) => {
                    ResponseTransport::Full(value_to_string(value).into_bytes())
                }
                PipelineData::ListStream(stream, _) => {
                    let (stream_tx, stream_rx) = tokio::sync::mpsc::channel(32);
                    for value in stream.into_inner() {
                        let s = value_to_string(value);
                        if stream_tx.blocking_send(s.into_bytes()).is_err() {
                            break;
                        }
                    }
                    ResponseTransport::Stream(stream_rx)
                }
                PipelineData::ByteStream(_, _) => todo!(),
            };

            let _ = body_tx.send((inferred_content_type, response));
            Ok(())
        });

        body_rx
    };

    // Get response metadata and body type. We use a select here to avoid blocking for metadata, if
    // the closure returns a pipeline without call .response
    let (meta, inferred_content_type, body) = tokio::select! {
        meta = meta_rx => (
            meta.unwrap_or(crate::Response { status: 200, headers: HashMap::new() }),
            None,
            None
        ),
        body = bridged_body => {
            let (content_type, response) = body?;
            (
                crate::Response { status: 200, headers: HashMap::new() },
                content_type,
                Some(response)
            )
        }
    };

    let body = match body {
        Some(b) => b,
        None => bridged_body.await?.1,
    };

    // Build response with appropriate headers
    let mut builder = hyper::Response::builder().status(meta.status);

    // Convert custom headers to HeaderMap
    let mut header_map = hyper::header::HeaderMap::new();
    for (k, v) in meta.headers {
        header_map.insert(
            hyper::header::HeaderName::from_bytes(k.as_bytes())?,
            hyper::header::HeaderValue::from_str(&v)?,
        );
    }

    if let Some(ct) = inferred_content_type {
        header_map.insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_str(&ct)?,
        );
    }

    *builder.headers_mut().unwrap() = header_map;

    // Set response body
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

    Ok(builder.body(body).unwrap())
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

fn value_to_string(value: Value) -> String {
    match value {
        Value::Nothing { .. } => String::new(),
        Value::String { val, .. } => val,
        Value::Int { val, .. } => val.to_string(),
        Value::Float { val, .. } => val.to_string(),
        Value::List { vals, .. } => {
            let items: Vec<String> = vals.iter().map(|v| value_to_string(v.clone())).collect();
            items.join("\n")
        }
        Value::Record { .. } => {
            serde_json::to_string(&value_to_json(&value)).unwrap_or_else(|_| String::new())
        }
        _ => todo!("value_to_string: {:?}", value),
    }
}

#[derive(Clone)]
pub struct ResponseStartCommand {
    tx: Arc<Mutex<Option<oneshot::Sender<crate::Response>>>>,
}

impl ResponseStartCommand {
    pub fn new(tx: oneshot::Sender<crate::Response>) -> Self {
        Self {
            tx: Arc::new(Mutex::new(Some(tx))),
        }
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
        let response = crate::Response { status, headers };

        let tx = self
            .tx
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| ShellError::GenericError {
                error: "Response already sent".into(),
                msg: "Channel was already consumed".into(),
                span: Some(call.head),
                help: None,
                inner: vec![],
            })?;

        tx.send(response).map_err(|_| ShellError::GenericError {
            error: "Failed to send response".into(),
            msg: "Channel closed".into(),
            span: Some(call.head),
            help: None,
            inner: vec![],
        })?;

        Ok(PipelineData::Empty)
    }
}
