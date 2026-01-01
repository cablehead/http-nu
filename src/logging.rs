use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Mutex, OnceLock};
use std::task::{Context, Poll};
use std::time::Instant;

use chrono::Local;
use hyper::body::{Body, Bytes, Frame, SizeHint};
use hyper::header::HeaderMap;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;

use crate::request::Request;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

// --- Event enum: zero-copy where possible ---

#[derive(Serialize)]
pub struct RequestData<'a> {
    pub proto: &'a str,
    pub method: &'a str,
    pub remote_ip: Option<String>,
    pub remote_port: Option<u16>,
    pub trusted_ip: Option<String>,
    pub headers: HashMap<String, String>,
    pub uri: String,
    pub path: &'a str,
    pub query: &'a HashMap<String, String>,
}

impl<'a> From<&'a Request> for RequestData<'a> {
    fn from(req: &'a Request) -> Self {
        Self {
            proto: &req.proto,
            method: req.method.as_str(),
            remote_ip: req.remote_ip.map(|ip| ip.to_string()),
            remote_port: req.remote_port,
            trusted_ip: req.trusted_ip.map(|ip| ip.to_string()),
            headers: req
                .headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect(),
            uri: req.uri.to_string(),
            path: &req.path,
            query: &req.query,
        }
    }
}

pub enum Event<'a> {
    // Access events
    Request {
        request_id: scru128::Scru128Id,
        method: &'a str,
        path: &'a str,
        trusted_ip: Option<String>,
        request: RequestData<'a>,
    },
    Response {
        request_id: scru128::Scru128Id,
        status: u16,
        headers: HashMap<String, String>,
        latency_ms: u64,
    },
    Complete {
        request_id: scru128::Scru128Id,
        bytes: u64,
        duration_ms: u64,
    },

    // Lifecycle events
    Started {
        address: &'a str,
        startup_ms: u64,
    },
    Reloaded,
    Error {
        error: &'a str,
    },
    Stopping {
        inflight: usize,
    },
    Stopped,
    StopTimedOut,
}

// --- Handler trait ---

pub trait Handler: Send + Sync {
    fn handle(&self, event: Event<'_>);
}

// --- Global handler ---

static HANDLER: OnceLock<Box<dyn Handler>> = OnceLock::new();

pub fn set_handler(handler: impl Handler + 'static) {
    let _ = HANDLER.set(Box::new(handler));
}

fn emit(event: Event<'_>) {
    if let Some(handler) = HANDLER.get() {
        handler.handle(event);
    }
}

// --- Public emit functions ---

pub fn log_request(request_id: scru128::Scru128Id, request: &Request) {
    emit(Event::Request {
        request_id,
        method: request.method.as_str(),
        path: &request.path,
        trusted_ip: request.trusted_ip.map(|ip| ip.to_string()),
        request: RequestData::from(request),
    });
}

pub fn log_response(
    request_id: scru128::Scru128Id,
    status: u16,
    headers: &HeaderMap,
    start_time: Instant,
) {
    let headers_map: HashMap<String, String> = headers
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();

    emit(Event::Response {
        request_id,
        status,
        headers: headers_map,
        latency_ms: start_time.elapsed().as_millis() as u64,
    });
}

pub fn log_complete(request_id: scru128::Scru128Id, bytes: u64, response_time: Instant) {
    emit(Event::Complete {
        request_id,
        bytes,
        duration_ms: response_time.elapsed().as_millis() as u64,
    });
}

pub fn log_started(address: &str, startup_ms: u128) {
    emit(Event::Started {
        address,
        startup_ms: startup_ms as u64,
    });
}

pub fn log_reloaded() {
    emit(Event::Reloaded);
}

pub fn log_error(error: &str) {
    emit(Event::Error { error });
}

pub fn log_stopping(inflight: usize) {
    emit(Event::Stopping { inflight });
}

pub fn log_stopped() {
    emit(Event::Stopped);
}

pub fn log_stop_timed_out() {
    emit(Event::StopTimedOut);
}

// --- JSONL handler ---

pub struct JsonlHandler;

impl Handler for JsonlHandler {
    fn handle(&self, event: Event<'_>) {
        let stamp = scru128::new().to_string();

        let json = match event {
            Event::Request {
                request_id,
                method,
                path,
                trusted_ip,
                request,
            } => {
                serde_json::json!({
                    "stamp": stamp,
                    "message": "request",
                    "request_id": request_id.to_string(),
                    "method": method,
                    "path": path,
                    "trusted_ip": trusted_ip,
                    "request": request,
                })
            }
            Event::Response {
                request_id,
                status,
                headers,
                latency_ms,
            } => {
                serde_json::json!({
                    "stamp": stamp,
                    "message": "response",
                    "request_id": request_id.to_string(),
                    "status": status,
                    "headers": headers,
                    "latency_ms": latency_ms,
                })
            }
            Event::Complete {
                request_id,
                bytes,
                duration_ms,
            } => {
                serde_json::json!({
                    "stamp": stamp,
                    "message": "complete",
                    "request_id": request_id.to_string(),
                    "bytes": bytes,
                    "duration_ms": duration_ms,
                })
            }
            Event::Started {
                address,
                startup_ms,
            } => {
                serde_json::json!({
                    "stamp": stamp,
                    "message": "started",
                    "address": address,
                    "startup_ms": startup_ms,
                })
            }
            Event::Reloaded => {
                serde_json::json!({
                    "stamp": stamp,
                    "message": "reloaded",
                })
            }
            Event::Error { error } => {
                serde_json::json!({
                    "stamp": stamp,
                    "message": "error",
                    "error": error,
                })
            }
            Event::Stopping { inflight } => {
                serde_json::json!({
                    "stamp": stamp,
                    "message": "stopping",
                    "inflight": inflight,
                })
            }
            Event::Stopped => {
                serde_json::json!({
                    "stamp": stamp,
                    "message": "stopped",
                })
            }
            Event::StopTimedOut => {
                serde_json::json!({
                    "stamp": stamp,
                    "message": "stop_timed_out",
                })
            }
        };

        println!("{}", serde_json::to_string(&json).unwrap_or_default());
    }
}

// --- Human-readable handler using indicatif ---

struct RequestState {
    pb: ProgressBar,
    method: String,
    path: String,
    trusted_ip: Option<String>,
    status: Option<u16>,
    latency_ms: Option<u64>,
}

pub struct HumanHandler {
    mp: MultiProgress,
    requests: Mutex<HashMap<String, RequestState>>,
}

impl HumanHandler {
    pub fn new() -> Self {
        Self {
            mp: MultiProgress::new(),
            requests: Mutex::new(HashMap::new()),
        }
    }

    fn truncate_middle(s: &str, max_len: usize) -> String {
        if s.len() <= max_len {
            return s.to_string();
        }
        let keep = (max_len - 3) / 2;
        format!("{}...{}", &s[..keep], &s[s.len() - keep..])
    }

    fn format_line(state: &RequestState, duration_ms: Option<u64>, bytes: Option<u64>) -> String {
        let timestamp = Local::now().format("%H:%M:%S%.3f");
        let ip = state.trusted_ip.as_deref().unwrap_or("-");
        let method = &state.method;
        let path = Self::truncate_middle(&state.path, 40);

        match (state.status, state.latency_ms, duration_ms, bytes) {
            // Complete: status, latency, duration, bytes
            (Some(status), Some(latency), Some(dur), Some(b)) => {
                format!(
                    "{timestamp} {ip:>15} {method:<6} {path:<40} {status} {latency:>6}ms {dur:>6}ms {b:>8}b"
                )
            }
            // Response: status, latency
            (Some(status), Some(latency), None, None) => {
                format!("{timestamp} {ip:>15} {method:<6} {path:<40} {status} {latency:>6}ms")
            }
            // Request pending
            _ => {
                format!("{timestamp} {ip:>15} {method:<6} {path:<40} ...")
            }
        }
    }
}

impl Default for HumanHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl Handler for HumanHandler {
    fn handle(&self, event: Event<'_>) {
        match event {
            Event::Started {
                address,
                startup_ms,
            } => {
                let version = env!("CARGO_PKG_VERSION");
                let pid = std::process::id();
                let now = Local::now().to_rfc2822();
                println!("<http-nu version=\"{version}\">");
                println!("     __  ,");
                println!(" .--()°'.'  pid {pid} · {address} · {startup_ms}ms 💜");
                println!("'|, . ,'    {now}");
                println!(" !_-(_\\");
            }
            Event::Reloaded => {
                println!("reloaded 🔄");
            }
            Event::Error { error } => {
                eprintln!("{error}");
            }
            Event::Stopping { inflight } => {
                println!("stopping, {inflight} connection(s) in flight...");
            }
            Event::Stopped => {
                println!("cu l8r </http-nu>");
            }
            Event::StopTimedOut => {
                println!("stop timed out, forcing exit");
            }
            Event::Request {
                request_id,
                method,
                path,
                trusted_ip,
                ..
            } => {
                let mut requests = self.requests.lock().unwrap();

                let pb = self.mp.add(ProgressBar::new_spinner());
                pb.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());

                let state = RequestState {
                    pb: pb.clone(),
                    method: method.to_string(),
                    path: path.to_string(),
                    trusted_ip,
                    status: None,
                    latency_ms: None,
                };
                pb.set_message(Self::format_line(&state, None, None));
                requests.insert(request_id.to_string(), state);
            }
            Event::Response {
                request_id,
                status,
                latency_ms,
                ..
            } => {
                let mut requests = self.requests.lock().unwrap();
                if let Some(state) = requests.get_mut(&request_id.to_string()) {
                    state.status = Some(status);
                    state.latency_ms = Some(latency_ms);
                    state.pb.set_message(Self::format_line(state, None, None));
                }
            }
            Event::Complete {
                request_id,
                bytes,
                duration_ms,
            } => {
                let mut requests = self.requests.lock().unwrap();
                if let Some(state) = requests.remove(&request_id.to_string()) {
                    state.pb.finish_with_message(Self::format_line(
                        &state,
                        Some(duration_ms),
                        Some(bytes),
                    ));
                }
            }
        }
    }
}

// --- LoggingBody wrapper ---

pub struct LoggingBody<B> {
    inner: B,
    request_id: scru128::Scru128Id,
    response_time: Instant,
    bytes_sent: u64,
    logged_complete: bool,
}

impl<B> LoggingBody<B> {
    pub fn new(inner: B, request_id: scru128::Scru128Id) -> Self {
        Self {
            inner,
            request_id,
            response_time: Instant::now(),
            bytes_sent: 0,
            logged_complete: false,
        }
    }

    fn do_log_complete(&mut self) {
        if !self.logged_complete {
            self.logged_complete = true;
            log_complete(self.request_id, self.bytes_sent, self.response_time);
        }
    }
}

impl<B> Body for LoggingBody<B>
where
    B: Body<Data = Bytes, Error = BoxError> + Unpin,
{
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let inner = Pin::new(&mut self.inner);
        match inner.poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    self.bytes_sent += data.len() as u64;
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => {
                self.do_log_complete();
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl<B> Drop for LoggingBody<B> {
    fn drop(&mut self) {
        self.do_log_complete();
    }
}
