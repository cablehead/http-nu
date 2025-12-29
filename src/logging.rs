use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Instant;

use chrono::Local;
use hyper::body::{Body, Bytes, Frame, SizeHint};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tracing::field::{Field, Visit};
use tracing::span::Attributes;
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::layer::Context as LayerContext;
use tracing_subscriber::Layer;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

// --- Tracing events ---

pub fn log_request(request_id: scru128::Scru128Id, method: &str, path: &str) {
    tracing::info!(
        target: "http_nu::access",
        message = "request",
        request_id = %request_id,
        method = method,
        path = path,
    );
}

pub fn log_response(
    request_id: scru128::Scru128Id,
    status: u16,
    headers: &hyper::header::HeaderMap,
    start_time: Instant,
) {
    tracing::info!(
        target: "http_nu::access",
        message = "response",
        request_id = %request_id,
        status = status,
        headers = ?headers,
        latency_ms = start_time.elapsed().as_millis() as u64,
    );
}

pub fn log_complete(request_id: scru128::Scru128Id, bytes: u64, start_time: Instant) {
    tracing::info!(
        target: "http_nu::access",
        message = "complete",
        request_id = %request_id,
        bytes = bytes,
        latency_ms = start_time.elapsed().as_millis() as u64,
    );
}

// --- JSONL layer with scru128 stamps ---

pub struct JsonlLayer;

impl JsonlLayer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for JsonlLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Subscriber> Layer<S> for JsonlLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: LayerContext<'_, S>) {
        if event.metadata().target() != "http_nu::access" {
            return;
        }

        let mut visitor = JsonVisitor::new();
        event.record(&mut visitor);

        visitor.map.insert(
            "stamp".to_string(),
            serde_json::Value::String(scru128::new().to_string()),
        );

        if let Ok(json) = serde_json::to_string(&visitor.map) {
            println!("{json}");
        }
    }
}

struct JsonVisitor {
    map: serde_json::Map<String, serde_json::Value>,
}

impl JsonVisitor {
    fn new() -> Self {
        Self {
            map: serde_json::Map::new(),
        }
    }
}

impl Visit for JsonVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let name = field.name();
        if name == "headers" {
            // Parse the debug output as header map
            self.map.insert(
                name.to_string(),
                serde_json::Value::String(format!("{value:?}")),
            );
        } else {
            let s = format!("{value:?}");
            self.map.insert(
                name.to_string(),
                serde_json::Value::String(s.trim_matches('"').to_string()),
            );
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.map.insert(
            field.name().to_string(),
            serde_json::Value::Number(value.into()),
        );
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.map.insert(
            field.name().to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
}

// --- Human-readable layer using indicatif ---

struct RequestState {
    pb: ProgressBar,
    method: String,
    path: String,
    status: Option<u16>,
    start_time: Instant,
}

pub struct HumanLayer {
    mp: MultiProgress,
    requests: Arc<Mutex<HashMap<String, RequestState>>>,
}

impl HumanLayer {
    pub fn new() -> Self {
        Self {
            mp: MultiProgress::new(),
            requests: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn format_line(state: &RequestState, bytes: Option<u64>) -> String {
        let timestamp = Local::now().format("%H:%M:%S%.3f");
        let status_part = state.status.map(|s| format!(" {s}")).unwrap_or_default();
        let timing = state.start_time.elapsed().as_millis();
        let bytes_part = bytes.map(|b| format!(" {b}b")).unwrap_or_default();
        let method = &state.method;
        let path = &state.path;

        if state.status.is_some() {
            format!("{timestamp} {method} {path}{status_part} {timing}ms{bytes_part}")
        } else {
            format!("{timestamp} {method} {path} ...")
        }
    }
}

impl Default for HumanLayer {
    fn default() -> Self {
        Self::new()
    }
}

// Visitor to extract fields from tracing events
struct FieldVisitor {
    request_id: Option<String>,
    message: Option<String>,
    method: Option<String>,
    path: Option<String>,
    status: Option<u16>,
    bytes: Option<u64>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self {
            request_id: None,
            message: None,
            method: None,
            path: None,
            status: None,
            bytes: None,
        }
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "request_id" => self.request_id = Some(format!("{value:?}")),
            "message" => self.message = Some(format!("{value:?}").trim_matches('"').to_string()),
            "method" => self.method = Some(format!("{value:?}").trim_matches('"').to_string()),
            "path" => self.path = Some(format!("{value:?}").trim_matches('"').to_string()),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        match field.name() {
            "status" => self.status = Some(value as u16),
            "bytes" => self.bytes = Some(value),
            _ => {}
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "request_id" => self.request_id = Some(value.to_string()),
            "message" => self.message = Some(value.to_string()),
            "method" => self.method = Some(value.to_string()),
            "path" => self.path = Some(value.to_string()),
            _ => {}
        }
    }
}

impl<S: Subscriber> Layer<S> for HumanLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: LayerContext<'_, S>) {
        if event.metadata().target() != "http_nu::access" {
            return;
        }

        let mut visitor = FieldVisitor::new();
        event.record(&mut visitor);

        let Some(request_id) = visitor.request_id else {
            return;
        };

        let mut requests = self.requests.lock().unwrap();

        match visitor.message.as_deref() {
            Some("request") => {
                let method = visitor.method.unwrap_or_default();
                let path = visitor.path.unwrap_or_default();

                let pb = self.mp.add(ProgressBar::new_spinner());
                pb.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());

                let state = RequestState {
                    pb: pb.clone(),
                    method,
                    path,
                    status: None,
                    start_time: Instant::now(),
                };
                pb.set_message(Self::format_line(&state, None));
                requests.insert(request_id, state);
            }
            Some("response") => {
                if let Some(state) = requests.get_mut(&request_id) {
                    state.status = visitor.status;
                    state.pb.set_message(Self::format_line(state, None));
                }
            }
            Some("complete") => {
                if let Some(state) = requests.remove(&request_id) {
                    state
                        .pb
                        .finish_with_message(Self::format_line(&state, visitor.bytes));
                }
            }
            _ => {}
        }
    }

    fn on_new_span(&self, _attrs: &Attributes<'_>, _id: &Id, _ctx: LayerContext<'_, S>) {}
}

// --- LoggingBody wrapper ---

pub struct LoggingBody<B> {
    inner: B,
    request_id: scru128::Scru128Id,
    start_time: Instant,
    bytes_sent: u64,
    logged_complete: bool,
}

impl<B> LoggingBody<B> {
    pub fn new(inner: B, request_id: scru128::Scru128Id, start_time: Instant) -> Self {
        Self {
            inner,
            request_id,
            start_time,
            bytes_sent: 0,
            logged_complete: false,
        }
    }

    fn do_log_complete(&mut self) {
        if !self.logged_complete {
            self.logged_complete = true;
            log_complete(self.request_id, self.bytes_sent, self.start_time);
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
