use std::collections::HashMap;
use std::io::{self, Write};
use std::pin::Pin;
use std::sync::OnceLock;
use std::task::{Context, Poll};
use std::time::Instant;

use chrono::Local;
use crossterm::{cursor, execute, terminal};
use hyper::body::{Body, Bytes, Frame, SizeHint};
use hyper::header::HeaderMap;
use serde::Serialize;
use tokio::sync::broadcast;

use crate::request::Request;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Datastar SDK version (matches CDN URL in stdlib)
pub const DATASTAR_VERSION: &str = "1.0-RC.7";

/// Startup options to display in preamble
#[derive(Clone, Default)]
pub struct StartupOptions {
    pub watch: bool,
    pub tls: Option<String>,
    pub store: Option<String>,
    pub topic: Option<String>,
    pub expose: Option<String>,
    pub services: bool,
}

// --- Token bucket rate limiter ---

struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_rate: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(capacity: f64, refill_rate: f64, now: Instant) -> Self {
        Self {
            tokens: capacity,
            capacity,
            refill_rate,
            last_refill: now,
        }
    }

    fn try_consume(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

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

#[derive(Clone)]
pub enum Event {
    // Access events
    Request {
        request_id: scru128::Scru128Id,
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
        options: StartupOptions,
    },
    Reloaded,
    Error {
        error: String,
    },
    Print {
        message: String,
    },
    Stopping {
        inflight: usize,
    },
    Stopped,
    StopTimedOut,
    Shutdown,
}

// --- Broadcast channel ---

static SENDER: OnceLock<broadcast::Sender<Event>> = OnceLock::new();

pub fn init_broadcast() -> broadcast::Receiver<Event> {
    let (tx, rx) = broadcast::channel(65536);
    let _ = SENDER.set(tx);
    rx
}

pub fn subscribe() -> Option<broadcast::Receiver<Event>> {
    SENDER.get().map(|tx| tx.subscribe())
}

fn emit(event: Event) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(event); // non-blocking, drops if no receivers
    }
}

// --- Public emit functions ---

pub fn log_request(request_id: scru128::Scru128Id, request: &Request) {
    emit(Event::Request {
        request_id,
        request: Box::new(RequestData::from(request)),
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

pub fn log_started(address: &str, startup_ms: u128, options: StartupOptions) {
    emit(Event::Started {
        address: address.to_string(),
        startup_ms: startup_ms as u64,
        options,
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

pub fn log_print(message: &str) {
    emit(Event::Print {
        message: message.to_string(),
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

pub fn shutdown() {
    emit(Event::Shutdown);
}

// --- JSONL handler (dedicated writer thread does serialization + IO) ---

pub fn run_jsonl_handler(rx: broadcast::Receiver<Event>) -> std::thread::JoinHandle<()> {
    use std::io::Write;

    std::thread::spawn(move || {
        let mut rx = rx;
        let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());

        loop {
            let event = match rx.blocking_recv() {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    let json = serde_json::json!({
                        "stamp": scru128::new().to_string(),
                        "message": "lagged",
                        "dropped": n,
                    });
                    if let Ok(line) = serde_json::to_string(&json) {
                        let _ = writeln!(stdout, "{line}");
                        let _ = stdout.flush();
                    }
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };

            if matches!(event, Event::Shutdown) {
                let _ = stdout.flush();
                break;
            }

            let needs_flush = matches!(
                &event,
                Event::Started { .. }
                    | Event::Stopped
                    | Event::StopTimedOut
                    | Event::Reloaded
                    | Event::Error { .. }
                    | Event::Print { .. }
            );

            let stamp = scru128::new().to_string();

            let json = match event {
                Event::Request {
                    request_id,
                    request,
                } => {
                    serde_json::json!({
                        "stamp": stamp,
                        "message": "request",
                        "request_id": request_id.to_string(),
                        "method": &request.method,
                        "path": &request.path,
                        "trusted_ip": &request.trusted_ip,
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
                    options,
                } => {
                    let xs_version: Option<&str> = if options.store.is_some() {
                        #[cfg(feature = "cross-stream")]
                        {
                            Some(env!("XS_VERSION"))
                        }
                        #[cfg(not(feature = "cross-stream"))]
                        {
                            None
                        }
                    } else {
                        None
                    };
                    serde_json::json!({
                        "stamp": stamp,
                        "message": "started",
                        "address": address,
                        "startup_ms": startup_ms,
                        "watch": options.watch,
                        "tls": options.tls,
                        "store": options.store,
                        "topic": options.topic,
                        "expose": options.expose,
                        "services": options.services,
                        "nu_version": env!("NU_VERSION"),
                        "xs_version": xs_version,
                        "datastar_version": DATASTAR_VERSION,
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
                Event::Print { message } => {
                    serde_json::json!({
                        "stamp": stamp,
                        "message": "print",
                        "content": message,
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
                Event::Shutdown => unreachable!(),
            };

            if let Ok(line) = serde_json::to_string(&json) {
                let _ = writeln!(stdout, "{line}");
            }

            // Flush if lifecycle event or channel is empty (idle)
            if needs_flush || rx.is_empty() {
                let _ = stdout.flush();
            }
        }

        let _ = stdout.flush();
    })
}

// --- Human-readable handler (dedicated thread) ---

struct RequestState {
    method: String,
    path: String,
    trusted_ip: Option<String>,
    start_time: Instant,
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

struct ActiveZone {
    stdout: io::Stdout,
    line_count: usize,
}

impl ActiveZone {
    fn new() -> Self {
        Self {
            stdout: io::stdout(),
            line_count: 0,
        }
    }

    /// Clear the active zone and move cursor back to start
    fn clear(&mut self) {
        if self.line_count > 0 {
            let _ = execute!(
                self.stdout,
                cursor::MoveUp(self.line_count as u16),
                terminal::Clear(terminal::ClearType::FromCursorDown)
            );
            self.line_count = 0;
        }
    }

    /// Print a permanent line (scrolls up, not part of active zone)
    fn print_permanent(&mut self, line: &str) {
        self.clear();
        println!("{line}");
        let _ = self.stdout.flush();
    }

    /// Redraw all active requests
    fn redraw(&mut self, active_ids: &[String], requests: &HashMap<String, RequestState>) {
        self.line_count = 0;
        if !active_ids.is_empty() {
            println!("· · ·  ✈ in flight  · · ·");
            self.line_count += 1;
            for id in active_ids {
                if let Some(state) = requests.get(id) {
                    let line = format_active_line(state);
                    println!("{line}");
                    self.line_count += 1;
                }
            }
        }
        let _ = self.stdout.flush();
    }
}

fn format_active_line(state: &RequestState) -> String {
    let timestamp = Local::now().format("%H:%M:%S%.3f");
    let ip = state.trusted_ip.as_deref().unwrap_or("-");
    let method = &state.method;
    let path = truncate_middle(&state.path, 40);
    let elapsed = state.start_time.elapsed().as_secs_f64();

    match (state.status, state.latency_ms) {
        (Some(status), Some(latency)) => {
            format!(
                "{timestamp} {ip:>15} {method:<6} {path:<40} {status} {latency:>6}ms {elapsed:>6.1}s"
            )
        }
        _ => {
            format!("{timestamp} {ip:>15} {method:<6} {path:<40} ... {elapsed:>6.1}s")
        }
    }
}

fn format_complete_line(state: &RequestState, duration_ms: u64, bytes: u64) -> String {
    let timestamp = Local::now().format("%H:%M:%S%.3f");
    let ip = state.trusted_ip.as_deref().unwrap_or("-");
    let method = &state.method;
    let path = truncate_middle(&state.path, 40);
    let status = state.status.unwrap_or(0);
    let latency = state.latency_ms.unwrap_or(0);

    format!(
        "{timestamp} {ip:>15} {method:<6} {path:<40} {status} {latency:>6}ms {duration_ms:>6}ms {bytes:>8}b"
    )
}

pub fn run_human_handler(rx: broadcast::Receiver<Event>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut rx = rx;
        let mut zone = ActiveZone::new();
        let mut requests: HashMap<String, RequestState> = HashMap::new();
        let mut active_ids: Vec<String> = Vec::new();

        // Rate limiting: token bucket (burst 40, refill 20/sec)
        let mut rate_limiter = TokenBucket::new(40.0, 20.0, Instant::now());
        let mut skipped: u64 = 0;
        let mut lagged: u64 = 0;

        loop {
            let event = match rx.blocking_recv() {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    lagged += n;
                    // Clear all pending - their Response/Complete events may have been dropped
                    requests.clear();
                    active_ids.clear();
                    zone.print_permanent(&format!(
                        "⚠ lagged: dropped {n} events, cleared in-flight"
                    ));
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => break,
            };
            match event {
                Event::Started {
                    address,
                    startup_ms,
                    options,
                } => {
                    let version = env!("CARGO_PKG_VERSION");
                    let pid = std::process::id();
                    let now = Local::now().to_rfc2822();
                    zone.print_permanent(&format!("<http-nu version=\"{version}\">"));
                    zone.print_permanent("     __  ,");
                    zone.print_permanent(&format!(
                        " .--()°'.'  pid {pid} · {address} · {startup_ms}ms 💜"
                    ));
                    zone.print_permanent(&format!("'|, . ,'    {now}"));

                    // Build options line: [http-nu opts] │ xs [store] [expose] [services]
                    let mut http_opts = Vec::new();
                    if options.watch {
                        http_opts.push("watch".to_string());
                    }
                    if let Some(ref tls) = options.tls {
                        http_opts.push(format!("tls:{tls}"));
                    }

                    let mut xs_opts = Vec::new();
                    if let Some(ref store) = options.store {
                        xs_opts.push(store.to_string());
                    }
                    if let Some(ref topic) = options.topic {
                        xs_opts.push(format!("topic:{topic}"));
                    }
                    if let Some(ref expose) = options.expose {
                        xs_opts.push(expose.to_string());
                    }
                    if options.services {
                        xs_opts.push("services".to_string());
                    }

                    // Build versions string
                    let mut versions = vec![format!("nu {}", env!("NU_VERSION"))];
                    #[cfg(feature = "cross-stream")]
                    if options.store.is_some() {
                        versions.push(format!("xs {}", env!("XS_VERSION")));
                    }
                    versions.push(format!("datastar {DATASTAR_VERSION}"));
                    let versions_str = versions.join(" · ");

                    let has_opts = !http_opts.is_empty() || !xs_opts.is_empty();
                    if has_opts {
                        // Options on duck line, versions on next line
                        let opts_str = match (http_opts.is_empty(), xs_opts.is_empty()) {
                            (false, true) => http_opts.join(" "),
                            (true, false) => format!("xs {}", xs_opts.join(" ")),
                            (false, false) => {
                                format!("{} │ xs {}", http_opts.join(" "), xs_opts.join(" "))
                            }
                            _ => unreachable!(),
                        };
                        zone.print_permanent(&format!(" !_-(_\\     {opts_str}"));
                        zone.print_permanent(&format!("            {versions_str}"));
                    } else {
                        // No options: versions on duck line
                        zone.print_permanent(&format!(" !_-(_\\     {versions_str}"));
                    }

                    zone.redraw(&active_ids, &requests);
                }
                Event::Reloaded => {
                    zone.print_permanent("reloaded 🔄");
                    zone.redraw(&active_ids, &requests);
                }
                Event::Error { error } => {
                    zone.clear();
                    eprintln!("ERROR: {error}");
                    zone.redraw(&active_ids, &requests);
                }
                Event::Print { message } => {
                    zone.print_permanent(&format!("PRINT: {message}"));
                    zone.redraw(&active_ids, &requests);
                }
                Event::Stopping { inflight } => {
                    zone.print_permanent(&format!(
                        "stopping, {inflight} connection(s) in flight..."
                    ));
                    zone.redraw(&active_ids, &requests);
                }
                Event::Stopped => {
                    let timestamp = Local::now().format("%H:%M:%S%.3f");
                    zone.print_permanent(&format!("{timestamp} cu l8r"));
                    zone.clear();
                    println!("</http-nu>");
                }
                Event::StopTimedOut => {
                    zone.print_permanent("stop timed out, forcing exit");
                }
                Event::Request {
                    request_id,
                    request,
                } => {
                    if !rate_limiter.try_consume(Instant::now()) {
                        skipped += 1;
                        continue;
                    }

                    // Print skip summary if any
                    if skipped > 0 {
                        zone.print_permanent(&format!("... skipped {skipped} requests"));
                        skipped = 0;
                    }

                    let id = request_id.to_string();
                    let state = RequestState {
                        method: request.method.clone(),
                        path: request.path.clone(),
                        trusted_ip: request.trusted_ip.clone(),
                        start_time: Instant::now(),
                        status: None,
                        latency_ms: None,
                    };
                    requests.insert(id.clone(), state);
                    active_ids.push(id);
                    zone.clear();
                    zone.redraw(&active_ids, &requests);
                }
                Event::Response {
                    request_id,
                    status,
                    latency_ms,
                    ..
                } => {
                    let id = request_id.to_string();
                    if let Some(state) = requests.get_mut(&id) {
                        state.status = Some(status);
                        state.latency_ms = Some(latency_ms);
                        zone.clear();
                        zone.redraw(&active_ids, &requests);
                    }
                }
                Event::Complete {
                    request_id,
                    bytes,
                    duration_ms,
                } => {
                    let id = request_id.to_string();
                    if let Some(state) = requests.remove(&id) {
                        active_ids.retain(|x| x != &id);
                        let line = format_complete_line(&state, duration_ms, bytes);
                        zone.print_permanent(&line);
                        zone.redraw(&active_ids, &requests);
                    }
                }
                Event::Shutdown => break,
            }
        }

        // Final summaries
        zone.clear();
        if skipped > 0 {
            println!("... skipped {skipped} requests");
        }
        if lagged > 0 {
            println!("⚠ total lagged: {lagged} events dropped");
        }
    })
}

// --- RequestGuard: ensures Complete fires even on abort ---

pub struct RequestGuard {
    request_id: scru128::Scru128Id,
    start: Instant,
    bytes_sent: u64,
}

impl RequestGuard {
    pub fn new(request_id: scru128::Scru128Id) -> Self {
        Self {
            request_id,
            start: Instant::now(),
            bytes_sent: 0,
        }
    }

    pub fn request_id(&self) -> scru128::Scru128Id {
        self.request_id
    }
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        log_complete(self.request_id, self.bytes_sent, self.start);
    }
}

// --- LoggingBody wrapper ---

pub struct LoggingBody<B> {
    inner: B,
    guard: RequestGuard,
}

impl<B> LoggingBody<B> {
    pub fn new(inner: B, guard: RequestGuard) -> Self {
        Self { inner, guard }
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
                    self.guard.bytes_sent += data.len() as u64;
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(e))) => Poll::Ready(Some(Err(e))),
            Poll::Ready(None) => Poll::Ready(None),
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

// No Drop impl needed - guard's Drop fires Complete

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn token_bucket_allows_burst() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(40.0, 20.0, start);

        // Should allow 40 requests immediately
        for _ in 0..40 {
            assert!(bucket.try_consume(start));
        }
        // 41st should fail
        assert!(!bucket.try_consume(start));
    }

    #[test]
    fn token_bucket_refills_over_time() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(40.0, 20.0, start);

        // Drain the bucket
        for _ in 0..40 {
            bucket.try_consume(start);
        }
        assert!(!bucket.try_consume(start));

        // After 100ms, should have 2 tokens (20/sec * 0.1s)
        let later = start + Duration::from_millis(100);
        assert!(bucket.try_consume(later));
        assert!(bucket.try_consume(later));
        assert!(!bucket.try_consume(later));
    }

    #[test]
    fn token_bucket_caps_at_capacity() {
        let start = Instant::now();
        let mut bucket = TokenBucket::new(40.0, 20.0, start);

        // Wait a long time - should still cap at 40
        let later = start + Duration::from_secs(10);
        for _ in 0..40 {
            assert!(bucket.try_consume(later));
        }
        assert!(!bucket.try_consume(later));
    }
}
