//! Build a `worker::Response` from the closure's `PipelineData`.
//!
//! Mirrors src/worker.rs's body assembly, minus the thread/mpsc bridge.
//! Three shapes:
//!   - Empty / Nothing            -> empty body
//!   - Value                      -> one-shot bytes
//!   - ListStream / ByteStream    -> streamed via `Response::from_stream`
//!
//! Content-type inference for non-streaming values lives in
//! `crate::response::infer_content_type` (shared with desktop). The
//! ListStream NDJSON-over-records detection is handled here because it
//! requires peeking the iterator.

use std::io::Read;

use futures_util::stream;
use nu_protocol::{PipelineData, Value};
use worker::Response;

use crate::engine::Engine;
use crate::response::{value_to_bytes, value_to_json};

/// Build the `$in` pipeline from the request body. Empty body -> Empty.
/// Non-empty -> single-shot ByteStream (the body is already buffered by
/// the time we get here; desktop streams from hyper via tokio mpsc).
pub(super) fn body_to_pipeline(body: Vec<u8>, engine: &Engine) -> PipelineData {
    if body.is_empty() {
        return PipelineData::Empty;
    }
    let span = nu_protocol::Span::unknown();
    let signals = engine.state.signals().clone();
    let mut consumed = false;
    let stream = nu_protocol::ByteStream::from_fn(
        span,
        signals,
        nu_protocol::ByteStreamType::Unknown,
        move |buffer: &mut Vec<u8>| {
            if !consumed {
                buffer.extend_from_slice(&body);
                consumed = true;
                Ok(true)
            } else {
                Ok(false)
            }
        },
    );
    PipelineData::ByteStream(stream, None)
}

/// Turn the closure's `PipelineData` into a `Response`. Returns the
/// response and an optional content-type override (set when we detect a
/// JSONL record stream and need `application/x-ndjson`).
pub(super) fn build_response(
    pd: PipelineData,
) -> std::result::Result<(Response, Option<String>), String> {
    match pd {
        PipelineData::Empty | PipelineData::Value(Value::Nothing { .. }, _) => {
            let resp = Response::empty().map_err(|e| format!("response: {e}"))?;
            Ok((resp, None))
        }
        PipelineData::Value(value, _) => {
            let bytes = value_to_bytes(value);
            let resp = Response::from_bytes(bytes).map_err(|e| format!("response: {e}"))?;
            Ok((resp, None))
        }
        PipelineData::ListStream(list_stream, _) => {
            // Peek the first value to detect JSONL mode, then chain the rest.
            let mut iter = list_stream.into_inner();
            let first = iter.next();
            let use_jsonl = first.as_ref().is_some_and(
                |v| matches!(v, Value::Record { val, .. } if val.get("__html").is_none()),
            );

            let chained = first.into_iter().chain(iter).map(move |value| {
                let bytes = if use_jsonl {
                    let mut line = serde_json::to_vec(&value_to_json(&value)).unwrap_or_default();
                    line.push(b'\n');
                    line
                } else {
                    value_to_bytes(value)
                };
                Ok::<Vec<u8>, worker::Error>(bytes)
            });

            let body_stream = stream::iter(chained);
            let resp = Response::from_stream(body_stream).map_err(|e| format!("stream: {e}"))?;
            let ct = use_jsonl.then(|| "application/x-ndjson".to_string());
            Ok((resp, ct))
        }
        PipelineData::ByteStream(byte_stream, _) => {
            // Pull raw bytes from the underlying reader on demand. 8KB
            // chunks mirror desktop's worker.rs.
            let reader = byte_stream
                .reader()
                .ok_or_else(|| "ByteStream has no reader".to_string())?;
            let body_stream = stream::unfold(reader, |mut reader| async move {
                let mut buf = vec![0u8; 8192];
                match reader.read(&mut buf) {
                    Ok(0) => None,
                    Ok(n) => {
                        buf.truncate(n);
                        Some((Ok::<Vec<u8>, worker::Error>(buf), reader))
                    }
                    Err(_) => None,
                }
            });
            let resp = Response::from_stream(body_stream).map_err(|e| format!("stream: {e}"))?;
            Ok((resp, None))
        }
    }
}
