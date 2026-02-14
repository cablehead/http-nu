use std::net::SocketAddr;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use arc_swap::ArcSwap;
use futures_util::StreamExt;
use http_body_util::{combinators::BoxBody, BodyExt, Empty, Full, StreamBody};
use hyper::body::{Bytes, Frame};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tower::Service;
use tower_http::services::{ServeDir, ServeFile};

use crate::compression;
use crate::logging::{log_request, log_response, LoggingBody, RequestGuard};
use crate::request::{resolve_trusted_ip, Request};
use crate::response::{Response, ResponseBodyType, ResponseTransport};
use crate::worker::{spawn_eval_thread, PipelineResult};

type BoxError = Box<dyn std::error::Error + Send + Sync>;
type HTTPResult = Result<hyper::Response<BoxBody<Bytes, BoxError>>, BoxError>;

const DATASTAR_JS_PATH: &str = "/datastar@1.0.0-RC.7.js";
const DATASTAR_JS: &[u8] = include_bytes!("stdlib/datastar/datastar@1.0.0-RC.7.js");

static DATASTAR_JS_BROTLI: OnceLock<Vec<u8>> = OnceLock::new();

fn get_datastar_js_brotli() -> &'static [u8] {
    DATASTAR_JS_BROTLI.get_or_init(|| compression::compress_full(DATASTAR_JS).unwrap())
}

pub async fn handle<B>(
    engine: Arc<ArcSwap<crate::Engine>>,
    addr: Option<SocketAddr>,
    trusted_proxies: Arc<Vec<ipnet::IpNet>>,
    datastar: Arc<bool>,
    req: hyper::Request<B>,
) -> Result<hyper::Response<BoxBody<Bytes, BoxError>>, BoxError>
where
    B: hyper::body::Body + Unpin + Send + 'static,
    B::Data: Into<Bytes> + Clone + Send,
    B::Error: Into<BoxError> + Send,
{
    // Load current engine snapshot - lock-free atomic operation
    let engine = engine.load_full();
    match handle_inner(engine, addr, trusted_proxies, datastar, req).await {
        Ok(response) => Ok(response),
        Err(err) => {
            eprintln!("Error handling request: {err}");
            let response = hyper::Response::builder().status(500).body(
                Full::new(format!("Script error: {err}").into())
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
    trusted_proxies: Arc<Vec<ipnet::IpNet>>,
    datastar: Arc<bool>,
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

    // Generate request ID and guard for logging
    let start_time = Instant::now();
    let request_id = scru128::new();
    let guard = RequestGuard::new(request_id);

    let remote_ip = addr.as_ref().map(|a| a.ip());
    let trusted_ip = resolve_trusted_ip(&parts.headers, remote_ip, &trusted_proxies);

    let request = Request {
        proto: format!("{:?}", parts.version),
        method: parts.method.clone(),
        authority: parts.uri.authority().map(|a| a.to_string()),
        remote_ip,
        remote_port: addr.as_ref().map(|a| a.port()),
        trusted_ip,
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

    // Phase 1: Log request
    log_request(request_id, &request);

    // Built-in route: serve embedded Datastar JS bundle (requires --datastar flag)
    if *datastar && request.path == DATASTAR_JS_PATH {
        let use_brotli = compression::accepts_brotli(&parts.headers);
        let mut header_map = hyper::header::HeaderMap::new();
        header_map.insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("application/javascript"),
        );
        header_map.insert(
            hyper::header::CACHE_CONTROL,
            hyper::header::HeaderValue::from_static("public, max-age=31536000, immutable"),
        );
        let body = if use_brotli {
            header_map.insert(
                hyper::header::CONTENT_ENCODING,
                hyper::header::HeaderValue::from_static("br"),
            );
            header_map.insert(
                hyper::header::VARY,
                hyper::header::HeaderValue::from_static("accept-encoding"),
            );
            Full::new(Bytes::from(get_datastar_js_brotli().to_vec()))
                .map_err(|never| match never {})
                .boxed()
        } else {
            Full::new(Bytes::from_static(DATASTAR_JS))
                .map_err(|never| match never {})
                .boxed()
        };
        log_response(request_id, 200, &header_map, start_time);
        let logging_body = LoggingBody::new(body, guard);
        let mut response = hyper::Response::builder()
            .status(200)
            .body(logging_body.boxed())?;
        *response.headers_mut() = header_map;
        return Ok(response);
    }

    let reload_token = engine.reload_token.clone();
    let (meta_rx, bridged_body) = spawn_eval_thread(engine, request, stream);

    // Wait for both:
    // 1. Special response (from .static or .reverse-proxy) - None if normal response
    // 2. Body pipeline result (includes http.response metadata for normal responses)
    let (special_response, body_result): (Option<Response>, Result<PipelineResult, BoxError>) =
        tokio::join!(async { meta_rx.await.ok() }, async {
            bridged_body.await.map_err(|e| e.into())
        });

    let use_brotli = compression::accepts_brotli(&parts.headers);

    // Check if we got a special response (.static or .reverse-proxy)
    match special_response.as_ref().map(|r| &r.body_type) {
        Some(ResponseBodyType::Normal) | None => {
            // Normal response - use metadata from pipeline
            build_normal_response(body_result?, use_brotli, guard, start_time, reload_token).await
        }
        Some(ResponseBodyType::Static {
            root,
            path,
            fallback,
        }) => {
            let mut static_req = hyper::Request::new(Empty::<Bytes>::new());
            *static_req.uri_mut() = format!("/{path}").parse().unwrap();
            *static_req.method_mut() = parts.method.clone();
            *static_req.headers_mut() = parts.headers.clone();

            let res = if let Some(fallback) = fallback {
                let fp = root.join(fallback);
                ServeDir::new(root)
                    .fallback(ServeFile::new(fp))
                    .call(static_req)
                    .await?
            } else {
                ServeDir::new(root).call(static_req).await?
            };
            let (res_parts, body) = res.into_parts();
            log_response(
                request_id,
                res_parts.status.as_u16(),
                &res_parts.headers,
                start_time,
            );

            let bytes = body.collect().await?.to_bytes();
            let inner_body = Full::new(bytes).map_err(|e| match e {}).boxed();
            let logging_body = LoggingBody::new(inner_body, guard);
            let res = hyper::Response::from_parts(res_parts, logging_body.boxed());
            Ok(res)
        }
        Some(ResponseBodyType::ReverseProxy {
            target_url,
            headers,
            preserve_host,
            strip_prefix,
            request_body,
            query,
        }) => {
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
            let target_uri = {
                let query_string = if let Some(custom_query) = query {
                    // Use custom query - convert HashMap to query string
                    url::form_urlencoded::Serializer::new(String::new())
                        .extend_pairs(custom_query.iter())
                        .finish()
                } else if let Some(orig_query) = parts.uri.query() {
                    // Use original query string
                    orig_query.to_string()
                } else {
                    String::new()
                };

                if query_string.is_empty() {
                    format!("{target_url}{path}")
                } else {
                    format!("{target_url}{path}?{query_string}")
                }
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
                let header_name = hyper::header::HeaderName::from_bytes(k.as_bytes())?;

                match v {
                    crate::response::HeaderValue::Single(s) => {
                        let header_value = hyper::header::HeaderValue::from_str(s)?;
                        header_map.insert(header_name, header_value);
                    }
                    crate::response::HeaderValue::Multiple(values) => {
                        for value in values {
                            if let Ok(header_value) = hyper::header::HeaderValue::from_str(value) {
                                header_map.append(header_name.clone(), header_value);
                            }
                        }
                    }
                }
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
                    let (res_parts, body) = response.into_parts();
                    log_response(
                        request_id,
                        res_parts.status.as_u16(),
                        &res_parts.headers,
                        start_time,
                    );

                    let inner_body = body.map_err(|e| e.into()).boxed();
                    let logging_body = LoggingBody::new(inner_body, guard);
                    let res = hyper::Response::from_parts(res_parts, logging_body.boxed());
                    Ok(res)
                }
                Err(_e) => {
                    let empty_headers = hyper::header::HeaderMap::new();
                    log_response(request_id, 502, &empty_headers, start_time);

                    let inner_body = Full::new("Bad Gateway".into())
                        .map_err(|never| match never {})
                        .boxed();
                    let logging_body = LoggingBody::new(inner_body, guard);
                    let response = hyper::Response::builder()
                        .status(502)
                        .body(logging_body.boxed())?;
                    Ok(response)
                }
            }
        }
    }
}

async fn build_normal_response(
    pipeline_result: PipelineResult,
    use_brotli: bool,
    guard: RequestGuard,
    start_time: Instant,
    reload_token: CancellationToken,
) -> HTTPResult {
    let request_id = guard.request_id();
    let (inferred_content_type, http_meta, body) = pipeline_result;
    let status = match (http_meta.status, &body) {
        (Some(s), _) => s,
        (None, ResponseTransport::Empty) => 204,
        (None, _) => 200,
    };
    let mut builder = hyper::Response::builder().status(status);
    let mut header_map = hyper::header::HeaderMap::new();

    // Content-type precedence:
    // 1. Explicit in http.response headers
    // 2. Pipeline metadata (from `to json`, etc.)
    // 3. Inferred: record->json, binary->octet-stream, list/stream of records->ndjson, empty->None
    // 4. Default: text/html
    let content_type = http_meta
        .headers
        .get("content-type")
        .or(http_meta.headers.get("Content-Type"))
        .and_then(|hv| match hv {
            crate::response::HeaderValue::Single(s) => Some(s.clone()),
            crate::response::HeaderValue::Multiple(v) => v.first().cloned(),
        })
        .or(inferred_content_type)
        .or_else(|| {
            if matches!(body, ResponseTransport::Empty) {
                None
            } else {
                Some("text/html; charset=utf-8".to_string())
            }
        });

    if let Some(ref ct) = content_type {
        header_map.insert(
            hyper::header::CONTENT_TYPE,
            hyper::header::HeaderValue::from_str(ct)?,
        );
    }

    // Add compression headers if using brotli
    if use_brotli {
        header_map.insert(
            hyper::header::CONTENT_ENCODING,
            hyper::header::HeaderValue::from_static("br"),
        );
        header_map.insert(
            hyper::header::VARY,
            hyper::header::HeaderValue::from_static("accept-encoding"),
        );
    }

    // Add SSE-required headers for event streams
    let is_sse = content_type.as_deref() == Some("text/event-stream");
    if is_sse {
        header_map.insert(
            hyper::header::CACHE_CONTROL,
            hyper::header::HeaderValue::from_static("no-cache"),
        );
        header_map.insert(
            hyper::header::CONNECTION,
            hyper::header::HeaderValue::from_static("keep-alive"),
        );
    }

    for (k, v) in &http_meta.headers {
        if k.to_lowercase() != "content-type" {
            let header_name = hyper::header::HeaderName::from_bytes(k.as_bytes())?;

            match v {
                crate::response::HeaderValue::Single(s) => {
                    let header_value = hyper::header::HeaderValue::from_str(s)?;
                    header_map.insert(header_name, header_value);
                }
                crate::response::HeaderValue::Multiple(values) => {
                    for value in values {
                        if let Ok(header_value) = hyper::header::HeaderValue::from_str(value) {
                            header_map.append(header_name.clone(), header_value);
                        }
                    }
                }
            }
        }
    }

    log_response(request_id, status, &header_map, start_time);
    *builder.headers_mut().unwrap() = header_map;

    let inner_body = match body {
        ResponseTransport::Empty => Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed(),
        ResponseTransport::Full(bytes) => {
            if use_brotli {
                let compressed = compression::compress_full(&bytes)?;
                Full::new(Bytes::from(compressed))
                    .map_err(|never| match never {})
                    .boxed()
            } else {
                Full::new(bytes.into())
                    .map_err(|never| match never {})
                    .boxed()
            }
        }
        ResponseTransport::Stream(rx) => {
            if use_brotli {
                compression::compress_stream(rx)
            } else if is_sse {
                // SSE streams abort on reload (error triggers client retry)
                let stream = futures_util::stream::try_unfold(
                    (ReceiverStream::new(rx), reload_token),
                    |(mut data_rx, token)| async move {
                        tokio::select! {
                            biased;
                            _ = token.cancelled() => {
                                Err(std::io::Error::other("reload").into())
                            }
                            item = StreamExt::next(&mut data_rx) => {
                                match item {
                                    Some(data) => Ok(Some((Frame::data(Bytes::from(data)), (data_rx, token)))),
                                    None => Ok(None),
                                }
                            }
                        }
                    },
                );
                BodyExt::boxed(StreamBody::new(stream))
            } else {
                let stream = ReceiverStream::new(rx).map(|data| Ok(Frame::data(Bytes::from(data))));
                BodyExt::boxed(StreamBody::new(stream))
            }
        }
    };

    // Wrap with LoggingBody for phase 3 (complete) logging
    let logging_body = LoggingBody::new(inner_body, guard);
    Ok(builder.body(logging_body.boxed())?)
}
