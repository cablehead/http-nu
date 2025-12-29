use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use hyper::body::{Body, Bytes, Frame, SizeHint};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

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
            let latency_ms = self.start_time.elapsed().as_secs_f64() * 1000.0;
            println!(
                "{}",
                serde_json::json!({
                    "stamp": scru128::new(),
                    "message": "complete",
                    "request_id": self.request_id,
                    "bytes": self.bytes_sent,
                    "latency_ms": latency_ms
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
        // Log complete if we haven't already (e.g., client disconnected early)
        self.log_complete();
    }
}
