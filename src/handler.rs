use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::body::{Bytes, Frame};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tower::Service;
use tower_http::services::ServeDir;

use crate::request::Request;
use crate::response::{Response, ResponseBodyType, ResponseTransport};
use crate::worker::spawn_eval_thread;

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type HTTPResult = Result<hyper::Response<BoxBody<Bytes, BoxError>>, BoxError>;

pub async fn handle<B>(
    engine: Arc<crate::Engine>,
    addr: Option<SocketAddr>,
    req: hyper::Request<B>,
) -> Result<hyper::Response<BoxBody<Bytes, BoxError>>, BoxError>
where
    B: hyper::body::Body + Unpin + Send + 'static,
    B::Data: Into<Bytes> + Clone + Send,
    B::Error: Into<BoxError> + Send,
{
    match handle_inner(engine, addr, req).await {
        Ok(response) => Ok(response),
        Err(err) => {
            eprintln!("Error handling request: {err}");
            let response = hyper::Response::builder().status(500).body(
                Full::new("Internal Server Error".into())
                    .map_err(|never| match never {})
                    .boxed(),
            )?;
            Ok(response)
        }
    }
}

async fn handle_inner<B>(
    engine: Arc<crate::Engine>,
    addr: Option<SocketAddr>,
    req: hyper::Request<B>,
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
            let mut static_req = hyper::Request::new(Empty::<Bytes>::new());
            *static_req.uri_mut() = format!("/{path}").parse().unwrap();
            *static_req.method_mut() = parts.method.clone();
            *static_req.headers_mut() = parts.headers.clone();

            let mut service = ServeDir::new(root);
            let res = service.call(static_req).await?;
            let (parts, body) = res.into_parts();
            let bytes = body.collect().await?.to_bytes();
            let res = hyper::Response::from_parts(
                parts,
                Full::new(bytes).map_err(|e| match e {}).boxed(),
            );
            Ok(res)
        }
        ResponseBodyType::ReverseProxy {
            target_url,
            headers,
            preserve_host,
            strip_prefix,
            request_body,
        } => {
            let body = Full::new(Bytes::from(request_body.clone()));
            let mut proxy_req = hyper::Request::new(body);

            // Handle strip_prefix
            let path = if let Some(prefix) = strip_prefix {
                parts
                    .uri
                    .path()
                    .strip_prefix(prefix)
                    .unwrap_or(parts.uri.path())
            } else {
                parts.uri.path()
            };

            // Build target URI
            let target_uri = if let Some(query) = parts.uri.query() {
                format!("{target_url}{path}?{query}")
            } else {
                format!("{target_url}{path}")
            };

            *proxy_req.uri_mut() = target_uri.parse().map_err(|e| Box::new(e) as BoxError)?;
            *proxy_req.method_mut() = parts.method.clone();

            // Copy original headers
            let mut header_map = parts.headers.clone();

            // Update Content-Length to match the new body
            if !request_body.is_empty() || header_map.contains_key(hyper::header::CONTENT_LENGTH) {
                header_map.insert(
                    hyper::header::CONTENT_LENGTH,
                    hyper::header::HeaderValue::from_str(&request_body.len().to_string())?,
                );
            }

            // Add custom headers
            for (k, v) in headers {
                header_map.insert(
                    hyper::header::HeaderName::from_bytes(k.as_bytes())?,
                    hyper::header::HeaderValue::from_str(v)?,
                );
            }

            // Handle preserve_host
            if !preserve_host {
                if let Ok(target_uri) = target_url.parse::<hyper::Uri>() {
                    if let Some(authority) = target_uri.authority() {
                        header_map.insert(
                            hyper::header::HOST,
                            hyper::header::HeaderValue::from_str(authority.as_ref())?,
                        );
                    }
                }
            }

            *proxy_req.headers_mut() = header_map;

            // Create a simple HTTP client and forward the request
            let client =
                hyper_util::client::legacy::Client::builder(hyper_util::rt::TokioExecutor::new())
                    .build_http();

            match client.request(proxy_req).await {
                Ok(response) => {
                    let (parts, body) = response.into_parts();
                    let bytes = body.collect().await?.to_bytes();
                    let res = hyper::Response::from_parts(
                        parts,
                        Full::new(bytes).map_err(|e| match e {}).boxed(),
                    );
                    Ok(res)
                }
                Err(_e) => {
                    let response = hyper::Response::builder().status(502).body(
                        Full::new("Bad Gateway".into())
                            .map_err(|never| match never {})
                            .boxed(),
                    )?;
                    Ok(response)
                }
            }
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
