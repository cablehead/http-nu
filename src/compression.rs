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
use tokio_stream::{Stream, StreamExt};

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
    S: Stream<Item = Result<Vec<u8>, BoxError>> + Unpin,
{
    type Item = Result<Frame<Bytes>, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.finished {
            return Poll::Ready(None);
        }

        // Drain ready chunks with PROCESS (lets brotli batch for compression),
        // then FLUSH once at the boundary -- when the source goes idle, ends,
        // or the per-poll budget is hit. One brotli sync-point per burst
        // instead of per chunk.
        const DRAIN_BUDGET: usize = 32;
        let mut accumulated: Vec<u8> = Vec::new();
        let mut drained = 0usize;

        loop {
            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    match self.encode(&chunk, BrotliEncoderOperation::BROTLI_OPERATION_PROCESS) {
                        Ok(out) => accumulated.extend_from_slice(&out),
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    }
                    drained += 1;
                    if drained >= DRAIN_BUDGET {
                        // Cap the loop so a chatty source can't starve other
                        // tasks. Flush what we have and yield.
                        match self.encode(&[], BrotliEncoderOperation::BROTLI_OPERATION_FLUSH) {
                            Ok(out) => accumulated.extend_from_slice(&out),
                            Err(e) => return Poll::Ready(Some(Err(e))),
                        }
                        cx.waker().wake_by_ref();
                        return Poll::Ready(Some(Ok(Frame::data(Bytes::from(accumulated)))));
                    }
                }

                // Source idle: flush accumulated data so the client gets it
                // immediately. If nothing is buffered, we're truly Pending --
                // the inner already registered our waker.
                Poll::Pending => {
                    match self.encode(&[], BrotliEncoderOperation::BROTLI_OPERATION_FLUSH) {
                        Ok(out) => accumulated.extend_from_slice(&out),
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    }
                    if accumulated.is_empty() {
                        return Poll::Pending;
                    }
                    return Poll::Ready(Some(Ok(Frame::data(Bytes::from(accumulated)))));
                }

                // Inner errored (e.g. SSE cancel): propagate the error, mark
                // ourselves finished so the next poll yields None. We do NOT
                // run the brotli FINISH op -- the deliberately truncated body
                // lets the client see this as a fetch error and auto-retry.
                Poll::Ready(Some(Err(e))) => {
                    self.finished = true;
                    return Poll::Ready(Some(Err(e)));
                }

                Poll::Ready(None) => {
                    self.finished = true;
                    match self.encode(&[], BrotliEncoderOperation::BROTLI_OPERATION_FINISH) {
                        Ok(out) => accumulated.extend_from_slice(&out),
                        Err(e) => return Poll::Ready(Some(Err(e))),
                    }
                    if accumulated.is_empty() {
                        return Poll::Ready(None);
                    }
                    return Poll::Ready(Some(Ok(Frame::data(Bytes::from(accumulated)))));
                }
            }
        }
    }
}

/// Wrap a streaming response body with brotli compression.
pub fn compress_stream(rx: mpsc::Receiver<Vec<u8>>) -> BoxBody<Bytes, BoxError> {
    let stream = ReceiverStream::new(rx).map(Ok::<Vec<u8>, BoxError>);
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

#[cfg(test)]
mod stream_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// Build a chunk of HTML-shaped text. Repeating structure so brotli gets
    /// a realistic ratio, unique ids so consecutive chunks are not identical.
    fn html_chunk(seed: usize, target_len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(target_len + 256);
        let mut i = 0usize;
        while out.len() < target_len {
            let s = format!(
                "<div class=\"pane\" id=\"p{seed}-{i}\" data-seq=\"{}\"><pre>line {i} of pane \
                 {seed}: the quick brown fox {} jumps over the lazy dog</pre></div>\n",
                seed * 1000 + i,
                (seed * 7919 + i * 104729) % 100003
            );
            out.extend_from_slice(s.as_bytes());
            i += 1;
        }
        out
    }

    /// Streaming decoder that drains everything the encoder has made
    /// decodable. The brotli decoder can report `NeedsMoreInput` while it
    /// still holds output that did not fit the caller's buffer, so keep
    /// calling with no input until a call comes back short. Draining here
    /// keeps the test measuring the encoder rather than the decode loop.
    struct Decoder {
        state: brotli::BrotliState<StandardAlloc, StandardAlloc, StandardAlloc>,
        out: Vec<u8>,
        finished: bool,
    }

    impl Decoder {
        fn new() -> Self {
            Self {
                state: brotli::BrotliState::new(
                    StandardAlloc::default(),
                    StandardAlloc::default(),
                    StandardAlloc::default(),
                ),
                out: Vec::new(),
                finished: false,
            }
        }

        fn feed(&mut self, input: &[u8]) {
            let mut avail_in = input.len();
            let mut in_off = 0usize;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                let mut avail_out = buf.len();
                let mut out_off = 0usize;
                let mut total = 0usize;
                let r = brotli::BrotliDecompressStream(
                    &mut avail_in,
                    &mut in_off,
                    input,
                    &mut avail_out,
                    &mut out_off,
                    &mut buf,
                    &mut total,
                    &mut self.state,
                );
                self.out.extend_from_slice(&buf[..out_off]);
                match r {
                    brotli::BrotliResult::NeedsMoreOutput => continue,
                    brotli::BrotliResult::NeedsMoreInput => {
                        if out_off == buf.len() {
                            continue;
                        }
                        break;
                    }
                    brotli::BrotliResult::ResultSuccess => {
                        self.finished = true;
                        break;
                    }
                    brotli::BrotliResult::ResultFailure => panic!("brotli decode failed"),
                }
            }
        }
    }

    /// Three large SSE-sized events arrive ~30ms apart from a source that
    /// then idles before closing. Each event must be fully decodable from the
    /// frames emitted by the time the source goes idle, not held inside the
    /// encoder; the whole stream must round-trip; and batching per burst must
    /// keep the ratio well above what a flush-per-write policy would give.
    #[tokio::test]
    async fn streaming_flush_releases_each_burst_promptly() {
        let chunks: Vec<Vec<u8>> = (0..3).map(|i| html_chunk(i, 120 * 1024)).collect();
        let expected: Vec<u8> = chunks.concat();
        let boundaries: Vec<usize> = chunks
            .iter()
            .scan(0usize, |acc, c| {
                *acc += c.len();
                Some(*acc)
            })
            .collect();

        let sent_at: Arc<Mutex<Vec<Instant>>> = Arc::new(Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel::<Vec<u8>>(64);

        let producer = {
            let chunks = chunks.clone();
            let sent_at = sent_at.clone();
            tokio::spawn(async move {
                for c in chunks {
                    sent_at.lock().unwrap().push(Instant::now());
                    tx.send(c).await.unwrap();
                    tokio::time::sleep(Duration::from_millis(30)).await;
                }
                // Source stays open but idle for a while, like a quiet SSE
                // stream, then closes.
                tokio::time::sleep(Duration::from_millis(300)).await;
                drop(tx);
            })
        };

        let inner = ReceiverStream::new(rx).map(Ok::<Vec<u8>, BoxError>);
        let mut stream = BrotliStream::new(inner);

        let mut decoder = Decoder::new();
        let mut compressed_total = 0usize;
        let mut arrived_at: Vec<Instant> = Vec::new();

        loop {
            let frame = tokio::time::timeout(Duration::from_secs(3), stream.next())
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "timed out waiting for compressed bytes; {} of {} chunks arrived",
                        arrived_at.len(),
                        chunks.len()
                    )
                });
            let Some(frame) = frame else { break };
            let data = frame
                .expect("stream errored")
                .into_data()
                .expect("data frame");
            compressed_total += data.len();
            decoder.feed(&data);
            assert!(
                expected.starts_with(&decoder.out),
                "decoded output diverged from input"
            );
            while arrived_at.len() < boundaries.len()
                && decoder.out.len() >= boundaries[arrived_at.len()]
            {
                arrived_at.push(Instant::now());
            }
        }
        producer.await.unwrap();

        assert!(decoder.finished, "stream ended without a brotli FINISH");
        assert_eq!(decoder.out, expected, "round trip mismatch");

        let sent_at = sent_at.lock().unwrap();
        assert_eq!(arrived_at.len(), chunks.len());
        let latencies: Vec<Duration> = arrived_at
            .iter()
            .zip(sent_at.iter())
            .map(|(a, s)| a.duration_since(*s))
            .collect();
        let ratio = expected.len() as f64 / compressed_total as f64;
        eprintln!("latencies: {latencies:?}; ratio {ratio:.1}x");
        for (i, l) in latencies.iter().enumerate() {
            assert!(
                *l < Duration::from_millis(200),
                "chunk {i} took {l:?} to come out of the encoder"
            );
        }
        // Measured ~14.7x for this synthetic input with one flush per burst.
        // A flush per small write would land far below this.
        assert!(ratio > 8.0, "compression ratio collapsed to {ratio:.1}x");
    }
}
