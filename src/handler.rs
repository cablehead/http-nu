use crate::request::Request;
use crate::response::{Response, ResponseBodyType, ResponseTransport};
use crate::worker::spawn_eval_thread;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::body::{Bytes, Frame};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tower::util::ServiceExt;
use tower_http::services::ServeDir;

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type HTTPResult = Result<hyper::Response<BoxBody<Bytes, BoxError>>, BoxError>;

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
            .unwrap_or_else(std::collections::HashMap::new),
    };

    println!(
        "{}",
        serde_json::json!({"stamp": scru128::new(), "message": "request", "meta": request})
    );

    let (meta_rx, bridged_body) = spawn_eval_thread(engine, request, stream);

    // Wait for both:
    // 1. Metadata - either from .response or default values when closure skips .response
    // 2. Body pipeline to start (but not necessarily complete as it may stream)
    let (meta, body_result): (
        Response,
        Result<(Option<String>, ResponseTransport), BoxError>,
    ) = tokio::join!(
        async {
            meta_rx.await.unwrap_or(Response {
                status: 200,
                headers: std::collections::HashMap::new(),
                body_type: ResponseBodyType::Normal,
            })
        },
        async { bridged_body.await.map_err(|e| e.into()) }
    );

    match &meta.body_type {
        ResponseBodyType::Normal => build_normal_response(&meta, Ok(body_result?)).await,
        ResponseBodyType::Static { root, path } => {
            let mut static_req = hyper::Request::new(axum::body::Body::empty());
            *static_req.uri_mut() = format!("/{path}").parse().unwrap();
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
