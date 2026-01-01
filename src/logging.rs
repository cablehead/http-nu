use std::collections::HashMap;
use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};
use std::time::Instant;

use chrono::Local;
use hyper::body::{Body, Bytes, Frame, SizeHint};
use hyper::header::HeaderMap;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;
use std::sync::mpsc;

use crate::request::Request;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

// --- Event enum: owned data for async broadcast ---

#[derive(Clone, Serialize)]
pub struct RequestData {
    pub proto: String,
    pub method: String,
    pub remote_ip: Option<String>,
    pub remote_port: Option<u16>,
    pub trusted_ip: Option<String>,
    pub headers: HashMap<String, String>,
    pub uri: String,
    pub path: String,
    pub query: HashMap<String, String>,
}

impl From<&Request> for RequestData {
    fn from(req: &Request) -> Self {
        Self {
            proto: req.proto.to_string(),
            method: req.method.to_string(),
            remote_ip: req.remote_ip.map(|ip| ip.to_string()),
            remote_port: req.remote_port,
            trusted_ip: req.trusted_ip.map(|ip| ip.to_string()),
            headers: req
                .headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect(),
            uri: req.uri.to_string(),
            path: req.path.clone(),
            query: req.query.clone(),
        }
    }
}

pub enum Event {
    // Access events
    Request {
        request_id: scru128::Scru128Id,
        method: String,
        path: String,
        trusted_ip: Option<String>,
        request: Box<RequestData>,
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
        address: String,
        startup_ms: u64,
    },
    Reloaded,
    Error {
        error: String,
    },
    Stopping {
        inflight: usize,
    },
    Stopped,
    StopTimedOut,
}

// --- mpsc channel ---

static SENDER: OnceLock<mpsc::Sender<Event>> = OnceLock::new();

pub fn init_channel() -> mpsc::Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    let _ = SENDER.set(tx);
    rx
}

fn emit(event: Event) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(event);
    }
}

// --- Public emit functions ---

pub fn log_request(request_id: scru128::Scru128Id, request: &Request) {
    let data = RequestData::from(request);
    emit(Event::Request {
        request_id,
        method: data.method.clone(),
        path: data.path.clone(),
        trusted_ip: data.trusted_ip.clone(),
        request: Box::new(data),
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
        address: address.to_string(),
        startup_ms: startup_ms as u64,
    });
}

pub fn log_reloaded() {
    emit(Event::Reloaded);
}

pub fn log_error(error: &str) {
    emit(Event::Error {
        error: error.to_string(),
    });
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

// --- JSONL handler (dedicated writer thread does serialization + IO) ---

pub fn run_jsonl_handler(rx: mpsc::Receiver<Event>) {
    use std::io::Write;

    std::thread::spawn(move || {
        let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());

        while let Ok(event) = rx.recv() {
            let needs_flush = matches!(
                &event,
                Event::Started { .. }
                    | Event::Stopped
                    | Event::StopTimedOut
                    | Event::Reloaded
                    | Event::Error { .. }
            );

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

            if let Ok(line) = serde_json::to_string(&json) {
                let _ = writeln!(stdout, "{line}");
            }

            // Flush on lifecycle events
            if needs_flush {
                let _ = stdout.flush();
            }
        }

        let _ = stdout.flush();
    });
}

// --- Human-readable handler (dedicated thread) ---

struct RequestState {
    pb: ProgressBar,
    method: String,
    path: String,
    trusted_ip: Option<String>,
    status: Option<u16>,
    latency_ms: Option<u64>,
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
    let path = truncate_middle(&state.path, 40);

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

pub fn run_human_handler(rx: mpsc::Receiver<Event>) {
    std::thread::spawn(move || {
        let mp = MultiProgress::new();
        let mut requests: HashMap<String, RequestState> = HashMap::new();

        // Rate limiting: ~10 requests/sec visible
        let min_interval = std::time::Duration::from_millis(100);
        let mut last_shown = std::time::Instant::now();
        let mut skipped: u64 = 0;

        while let Ok(event) = rx.recv() {
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
                    let now = std::time::Instant::now();
                    if now.duration_since(last_shown) < min_interval {
                        skipped += 1;
                        continue;
                    }
                    last_shown = now;

                    // Print skip summary if any
                    if skipped > 0 {
                        println!("... skipped {skipped} requests");
                        skipped = 0;
                    }

                    let pb = mp.add(ProgressBar::new_spinner());
                    pb.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());

                    let state = RequestState {
                        pb: pb.clone(),
                        method,
                        path,
                        trusted_ip,
                        status: None,
                        latency_ms: None,
                    };
                    pb.set_message(format_line(&state, None, None));
                    requests.insert(request_id.to_string(), state);
                }
                Event::Response {
                    request_id,
                    status,
                    latency_ms,
                    ..
                } => {
                    if let Some(state) = requests.get_mut(&request_id.to_string()) {
                        state.status = Some(status);
                        state.latency_ms = Some(latency_ms);
                        state.pb.set_message(format_line(state, None, None));
                    }
                }
                Event::Complete {
                    request_id,
                    bytes,
                    duration_ms,
                } => {
                    if let Some(state) = requests.remove(&request_id.to_string()) {
                        state.pb.finish_with_message(format_line(
                            &state,
                            Some(duration_ms),
                            Some(bytes),
                        ));
                    }
                }
            }
        }

        // Final skip summary
        if skipped > 0 {
            println!("... skipped {skipped} requests");
        }
    });
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
