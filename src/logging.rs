use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use hyper::body::{Body, Bytes, Frame, SizeHint};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub fn log_response(
    request_id: scru128::Scru128Id,
    status: u16,
    headers: &hyper::header::HeaderMap,
    start_time: Instant,
) {
    println!(
        "{}",
        serde_json::json!({
            "stamp": scru128::new(),
            "message": "response",
            "request_id": request_id,
            "status": status,
            "headers": header_map_to_json(headers),
            "latency_ms": start_time.elapsed().as_millis()
        })
    );
}

fn header_map_to_json(headers: &hyper::header::HeaderMap) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (name, value) in headers.iter() {
        let key = name.as_str().to_string();
        let val = value.to_str().unwrap_or("<binary>").to_string();
        if let Some(existing) = map.get_mut(&key) {
            match existing {
                serde_json::Value::Array(arr) => arr.push(serde_json::Value::String(val)),
                serde_json::Value::String(s) => {
                    *existing = serde_json::Value::Array(vec![
                        serde_json::Value::String(s.clone()),
                        serde_json::Value::String(val),
                    ]);
                }
                _ => {}
            }
        } else {
            map.insert(key, serde_json::Value::String(val));
        }
    }
    serde_json::Value::Object(map)
}

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

    fn log_complete(&mut self) {
        if !self.logged_complete {
            self.logged_complete = true;
            println!(
                "{}",
                serde_json::json!({
                    "stamp": scru128::new(),
                    "message": "complete",
                    "request_id": self.request_id,
                    "bytes": self.bytes_sent,
                    "latency_ms": self.start_time.elapsed().as_millis()
                })
            );
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
                self.log_complete();
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
        self.log_complete();
    }
}
