use brotli::enc::backward_references::BrotliEncoderParams;
use brotli::enc::encode::{BrotliEncoderOperation, BrotliEncoderStateStruct};
use brotli::enc::StandardAlloc;
use bytes::Bytes;
use headers::Header;
use http_body_util::{combinators::BoxBody, BodyExt, StreamBody};
use http_encoding_headers::{AcceptEncoding, Encoding};
use hyper::body::Frame;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

const BROTLI_QUALITY: i32 = 4;
const OUTBUF_CAP: usize = 16 * 1024;

/// Check if the request accepts brotli encoding.
///
/// Parses the `Accept-Encoding` header respecting quality values.
/// Returns `true` only if `br` is present with quality > 0.
#[must_use]
pub fn accepts_brotli(headers: &hyper::header::HeaderMap) -> bool {
    let Ok(accept) =
        AcceptEncoding::decode(&mut headers.get_all(hyper::header::ACCEPT_ENCODING).iter())
    else {
        return false;
    };
    accept.preferred_allowed([Encoding::Br].iter()).is_some()
}

/// A streaming brotli compressor that flushes per chunk.
pub struct BrotliStream<S> {
    inner: S,
    encoder: BrotliEncoderStateStruct<StandardAlloc>,
    out_scratch: Vec<u8>,
    tmp: Vec<u8>,
    finished: bool,
}

impl<S> BrotliStream<S> {
    pub fn new(inner: S) -> Self {
        let params = BrotliEncoderParams {
            quality: BROTLI_QUALITY,
            ..Default::default()
        };

        let mut encoder = BrotliEncoderStateStruct::new(StandardAlloc::default());
        encoder.params = params;

        Self {
            inner,
            encoder,
            out_scratch: Vec::with_capacity(OUTBUF_CAP),
            tmp: vec![0u8; OUTBUF_CAP],
            finished: false,
        }
    }

    /// Unified Brotli driver for PROCESS/FLUSH/FINISH.
    fn encode(&mut self, input: &[u8], op: BrotliEncoderOperation) -> Result<Bytes, BoxError> {
        self.out_scratch.clear();
        let mut in_offset = 0usize;

        loop {
            let mut avail_in = input.len().saturating_sub(in_offset);
            let mut avail_out = self.tmp.len();
            let mut out_offset = 0usize;

            let ok = self.encoder.compress_stream(
                op,
                &mut avail_in,
                &input[in_offset..],
                &mut in_offset,
                &mut avail_out,
                &mut self.tmp,
                &mut out_offset,
                &mut None,
                &mut |_, _, _, _| (),
            );

            if !ok {
                return Err("brotli compression failed".into());
            }

            if out_offset > 0 {
                self.out_scratch.extend_from_slice(&self.tmp[..out_offset]);
            }

            let done = match op {
                BrotliEncoderOperation::BROTLI_OPERATION_FINISH => self.encoder.is_finished(),
                BrotliEncoderOperation::BROTLI_OPERATION_FLUSH => !self.encoder.has_more_output(),
                BrotliEncoderOperation::BROTLI_OPERATION_PROCESS => {
                    in_offset >= input.len() && !self.encoder.has_more_output()
                }
                _ => unreachable!("unexpected Brotli operation"),
            };

            if done {
                break;
            }
        }

        // Take ownership while preserving capacity for next call
        let result = std::mem::replace(&mut self.out_scratch, Vec::with_capacity(OUTBUF_CAP));
        Ok(Bytes::from(result))
    }
}

impl<S> Stream for BrotliStream<S>
where
    S: Stream<Item = Vec<u8>> + Unpin,
{
    type Item = Result<Frame<Bytes>, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }

        match Pin::new(&mut self.inner).poll_next(cx) {
            Poll::Ready(Some(chunk)) => {
                match self.encode(&chunk, BrotliEncoderOperation::BROTLI_OPERATION_FLUSH) {
                    Ok(compressed) => {
                        if compressed.is_empty() {
                            // FLUSH on non-empty input should always produce output,
                            // but handle defensively
                            cx.waker().wake_by_ref();
                            Poll::Pending
                        } else {
                            Poll::Ready(Some(Ok(Frame::data(compressed))))
                        }
                    }
                    Err(e) => Poll::Ready(Some(Err(e))),
                }
            }

            Poll::Ready(None) => {
                self.finished = true;
                match self.encode(&[], BrotliEncoderOperation::BROTLI_OPERATION_FINISH) {
                    Ok(final_data) => {
                        if final_data.is_empty() {
                            Poll::Ready(None)
                        } else {
                            Poll::Ready(Some(Ok(Frame::data(final_data))))
                        }
                    }
                    Err(e) => Poll::Ready(Some(Err(e))),
                }
            }

            Poll::Pending => Poll::Pending,
        }
    }
}

/// Wrap a streaming response body with brotli compression.
pub fn compress_stream(rx: mpsc::Receiver<Vec<u8>>) -> BoxBody<Bytes, BoxError> {
    let stream = ReceiverStream::new(rx);
    let brotli_stream = BrotliStream::new(stream);
    StreamBody::new(brotli_stream).boxed()
}

/// Compress an entire body eagerly.
pub fn compress_full(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut output = Vec::new();
    let params = BrotliEncoderParams {
        quality: BROTLI_QUALITY,
        ..Default::default()
    };
    brotli::BrotliCompress(&mut &*data, &mut output, &params)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::{HeaderMap, HeaderValue, ACCEPT_ENCODING};

    #[test]
    fn test_accepts_brotli_simple() {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT_ENCODING,
            HeaderValue::from_static("gzip, deflate, br"),
        );
        assert!(accepts_brotli(&headers));
    }

    #[test]
    fn test_rejects_brotli_quality_zero() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, br;q=0"));
        assert!(!accepts_brotli(&headers));
    }

    #[test]
    fn test_no_brotli() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("gzip, deflate"));
        assert!(!accepts_brotli(&headers));
    }

    #[test]
    fn test_no_accept_encoding_header() {
        let headers = HeaderMap::new();
        assert!(!accepts_brotli(&headers));
    }

    #[test]
    fn test_brotli_only() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT_ENCODING, HeaderValue::from_static("br"));
        assert!(accepts_brotli(&headers));
    }
}
