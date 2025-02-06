use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Read;
use std::net::SocketAddr;

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

#[derive(Debug)]
enum ResponseTransport {
    Empty,
    Full(Vec<u8>),
    Stream(mpsc::Receiver<Vec<u8>>),
}

pub async fn handle<B>(
    engine: crate::Engine,
    addr: Option<SocketAddr>,
    req: Request<B>,
) -> Result<Response<BoxBody<Bytes, BoxError>>, BoxError>
where
    B: hyper::body::Body + Unpin + Send + 'static,
    B::Data: Into<Bytes> + Clone + Send,
    B::Error: Into<BoxError> + Send,
{
    match handle_inner(engine, addr, req).await {
        Ok(response) => Ok(response),
        Err(err) => {
            eprintln!("Error handling request: {}", err);
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
    engine: crate::Engine,
    addr: Option<SocketAddr>,
    req: Request<B>,
) -> HTTPResult
where
    B: hyper::body::Body + Unpin + Send + 'static,
    B::Data: Into<Bytes> + Clone + Send,
    B::Error: Into<BoxError> + Send,
{
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

    let (meta_rx, bridged_body) = {
        let (meta_tx, meta_rx) = tokio::sync::oneshot::channel();
        let (body_tx, body_rx) = tokio::sync::oneshot::channel();

        std::thread::spawn(move || -> Result<(), BoxError> {
            RESPONSE_TX.with(|tx| {
                *tx.borrow_mut() = Some(meta_tx);
            });

            let result = engine.eval(request, stream.into());

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
                        ResponseTransport::Full(value_to_string(value).into_bytes()),
                    ));
                    Ok(())
                }
                PipelineData::ListStream(stream, _) => {
                    let (stream_tx, stream_rx) = tokio::sync::mpsc::channel(32);
                    let _ =
                        body_tx.send((inferred_content_type, ResponseTransport::Stream(stream_rx)));

                    for value in stream.into_inner() {
                        if stream_tx
                            .blocking_send(value_to_string(value).into_bytes())
                            .is_err()
                        {
                            break;
                        }
                    }
                    Ok(())
                }

                PipelineData::ByteStream(stream, meta) => {
                    let (stream_tx, stream_rx) = tokio::sync::mpsc::channel(32);
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
        });

        (meta_rx, body_rx)
    };

    // Wait for both:
    // 1. Metadata - either from .response or default values when closure skips .response
    // 2. Body pipeline to start (but not necessarily complete as it may stream)
    let (meta, body_result) = tokio::join!(
        async {
            meta_rx.await.unwrap_or(crate::Response {
                status: 200,
                headers: HashMap::new(),
            })
        },
        bridged_body
    );
    let (inferred_content_type, body) = body_result?;

    // Build response with appropriate headers
    let mut builder = hyper::Response::builder().status(meta.status);
    let mut header_map = hyper::header::HeaderMap::new();

    // First apply content-type from .response headers if present
    let content_type = meta
        .headers
        .get("content-type")
        .or(meta.headers.get("Content-Type"))
        .cloned()
        // Then pipeline metadata
        .or(inferred_content_type)
        // Finally default
        .unwrap_or("text/html; charset=utf-8".to_string());

    header_map.insert(
        hyper::header::CONTENT_TYPE,
        hyper::header::HeaderValue::from_str(&content_type)?,
    );

    // Add rest of custom headers
    for (k, v) in meta.headers {
        if k.to_lowercase() != "content-type" {
            header_map.insert(
                hyper::header::HeaderName::from_bytes(k.as_bytes())?,
                hyper::header::HeaderValue::from_str(&v)?,
            );
        }
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
        let response = crate::Response { status, headers };

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
    static RESPONSE_TX: RefCell<Option<oneshot::Sender<crate::Response>>> = const { RefCell::new(None) };
}
