use brotli::enc::backward_references::BrotliEncoderParams;
use brotli::enc::encode::{BrotliEncoderOperation, BrotliEncoderStateStruct};
use brotli::enc::StandardAlloc;
use bytes::Bytes;
use http_body_util::{combinators::BoxBody, BodyExt, StreamBody};
use hyper::body::Frame;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::Stream;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Check if the request accepts brotli encoding
pub fn accepts_brotli(headers: &hyper::header::HeaderMap) -> bool {
    headers
        .get(hyper::header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(',').any(|part| part.trim().starts_with("br")))
        .unwrap_or(false)
}

/// A streaming brotli compressor that flushes on every chunk.
/// This ensures SSE and other streaming responses get data to clients immediately.
pub struct BrotliStream<S> {
    inner: S,
    encoder: BrotliEncoderStateStruct<StandardAlloc>,
    finished: bool,
}

impl<S> BrotliStream<S> {
    pub fn new(inner: S) -> Self {
        // Use quality 4 (same as tower-http default for streaming)
        // Lower quality = faster compression, less latency
        let params = BrotliEncoderParams {
            quality: 4,
            ..Default::default()
        };

        let mut encoder = BrotliEncoderStateStruct::new(StandardAlloc::default());
        encoder.params = params;

        Self {
            inner,
            encoder,
            finished: false,
        }
    }

    fn compress_and_flush(&mut self, input: &[u8]) -> Vec<u8> {
        let mut output = vec![0u8; input.len() + 1024]; // Extra space for compression overhead
        let mut input_offset = 0;
        let mut output_offset = 0;
        let mut total_output = Vec::new();

        // First, process all input
        while input_offset < input.len() {
            let mut available_in = input.len() - input_offset;
            let mut available_out = output.len() - output_offset;

            self.encoder.compress_stream(
                BrotliEncoderOperation::BROTLI_OPERATION_PROCESS,
                &mut available_in,
                &input[input_offset..],
                &mut input_offset,
                &mut available_out,
                &mut output,
                &mut output_offset,
                &mut None,
                &mut |_, _, _, _| (),
            );

            if output_offset > 0 {
                total_output.extend_from_slice(&output[..output_offset]);
                output_offset = 0;
            }
        }

        // Then flush to ensure all compressed data is emitted
        loop {
            let mut available_in = 0;
            let mut available_out = output.len();
            let mut dummy_offset = 0;

            self.encoder.compress_stream(
                BrotliEncoderOperation::BROTLI_OPERATION_FLUSH,
                &mut available_in,
                &[],
                &mut dummy_offset,
                &mut available_out,
                &mut output,
                &mut output_offset,
                &mut None,
                &mut |_, _, _, _| (),
            );

            if output_offset > 0 {
                total_output.extend_from_slice(&output[..output_offset]);
                output_offset = 0;
            }

            if !self.encoder.has_more_output() {
                break;
            }
        }

        total_output
    }

    fn finish(&mut self) -> Vec<u8> {
        let mut output = vec![0u8; 1024];
        let mut output_offset = 0;
        let mut total_output = Vec::new();

        loop {
            let mut available_in = 0;
            let mut available_out = output.len();
            let mut dummy_offset = 0;

            self.encoder.compress_stream(
                BrotliEncoderOperation::BROTLI_OPERATION_FINISH,
                &mut available_in,
                &[],
                &mut dummy_offset,
                &mut available_out,
                &mut output,
                &mut output_offset,
                &mut None,
                &mut |_, _, _, _| (),
            );

            if output_offset > 0 {
                total_output.extend_from_slice(&output[..output_offset]);
                output_offset = 0;
            }

            if self.encoder.is_finished() {
                break;
            }
        }

        total_output
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
            Poll::Ready(Some(data)) => {
                let compressed = self.compress_and_flush(&data);
                if compressed.is_empty() {
                    // Re-poll if compression produced nothing (shouldn't happen with flush)
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(Some(Ok(Frame::data(Bytes::from(compressed)))))
                }
            }
            Poll::Ready(None) => {
                // Stream ended, finalize compression
                self.finished = true;
                let final_data = self.finish();
                if final_data.is_empty() {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Ok(Frame::data(Bytes::from(final_data)))))
                }
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Wrap a streaming response body with brotli compression
pub fn compress_stream(rx: mpsc::Receiver<Vec<u8>>) -> BoxBody<Bytes, BoxError> {
    let stream = ReceiverStream::new(rx);
    let brotli_stream = BrotliStream::new(stream);
    StreamBody::new(brotli_stream).boxed()
}

/// Compress a full body with brotli (quality 4 for speed)
pub fn compress_full(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let params = BrotliEncoderParams {
        quality: 4,
        ..Default::default()
    };
    brotli::BrotliCompress(&mut &data[..], &mut output, &params).unwrap();
    output
}
