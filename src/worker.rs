use crate::commands::RESPONSE_TX;
use crate::logging::log_error;
use crate::request::{request_to_value, Request};
use crate::response::{
    extract_http_response_meta, value_to_bytes, value_to_json, HttpResponseMeta, Response,
    ResponseTransport,
};
use nu_protocol::{
    engine::{Job, StateWorkingSet, ThreadJob},
    format_cli_error, PipelineData, Value,
};
use std::io::Read;
use std::sync::{mpsc, Arc};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

/// Check if a value is a record without __html field
fn is_jsonl_record(value: &Value) -> bool {
    matches!(value, Value::Record { val, .. } if val.get("__html").is_none())
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Result of pipeline evaluation containing content-type, HTTP response metadata, and body
pub type PipelineResult = (Option<String>, HttpResponseMeta, ResponseTransport);

pub fn spawn_eval_thread(
    engine: Arc<crate::Engine>,
    request: Request,
    stream: nu_protocol::ByteStream,
) -> (
    oneshot::Receiver<Response>,
    oneshot::Receiver<PipelineResult>,
) {
    let (meta_tx, meta_rx) = tokio::sync::oneshot::channel();
    let (body_tx, body_rx) = tokio::sync::oneshot::channel();

    fn inner(
        engine: Arc<crate::Engine>,
        request: Request,
        stream: nu_protocol::ByteStream,
        meta_tx: oneshot::Sender<Response>,
        body_tx: oneshot::Sender<PipelineResult>,
    ) -> Result<(), BoxError> {
        RESPONSE_TX.with(|tx| {
            *tx.borrow_mut() = Some(meta_tx);
        });
        let result = engine.run_closure(
            request_to_value(&request, nu_protocol::Span::unknown()),
            stream.into(),
        );
        // Always clear the thread local storage after eval completes
        RESPONSE_TX.with(|tx| {
            let _ = tx.borrow_mut().take(); // This will drop the sender if it wasn't used
        });
        let output = result?;
        let inferred_content_type = match &output {
            PipelineData::Value(Value::Record { val, .. }, meta)
                if meta.as_ref().and_then(|m| m.content_type.clone()).is_none() =>
            {
                if val.get("__html").is_some() {
                    Some("text/html; charset=utf-8".to_string())
                } else {
                    Some("application/json".to_string())
                }
            }
            PipelineData::Value(Value::List { vals, .. }, meta)
                if meta.as_ref().and_then(|m| m.content_type.clone()).is_none()
                    && !vals.is_empty()
                    && vals.iter().all(is_jsonl_record) =>
            {
                Some("application/x-ndjson".to_string())
            }
            PipelineData::Value(Value::Binary { .. }, meta)
                if meta.as_ref().and_then(|m| m.content_type.clone()).is_none() =>
            {
                Some("application/octet-stream".to_string())
            }
            PipelineData::Value(_, meta) | PipelineData::ListStream(_, meta) => {
                meta.as_ref().and_then(|m| m.content_type.clone())
            }
            _ => None,
        };
        match output {
            PipelineData::Empty => {
                let _ = body_tx.send((
                    inferred_content_type,
                    HttpResponseMeta::default(),
                    ResponseTransport::Empty,
                ));
                Ok(())
            }
            PipelineData::Value(Value::Nothing { .. }, meta) => {
                let http_meta = extract_http_response_meta(meta.as_ref());
                let _ = body_tx.send((inferred_content_type, http_meta, ResponseTransport::Empty));
                Ok(())
            }
            PipelineData::Value(Value::Error { error, .. }, _) => {
                let working_set = StateWorkingSet::new(&engine.state);
                Err(format_cli_error(&working_set, error.as_ref(), None).into())
            }
            PipelineData::Value(Value::List { vals, .. }, meta)
                if !vals.is_empty() && vals.iter().all(is_jsonl_record) =>
            {
                let http_meta = extract_http_response_meta(meta.as_ref());
                // JSONL: each record as JSON line
                let jsonl: Vec<u8> = vals
                    .into_iter()
                    .flat_map(|v| {
                        let mut line = serde_json::to_vec(&value_to_json(&v)).unwrap_or_default();
                        line.push(b'\n');
                        line
                    })
                    .collect();
                let _ = body_tx.send((
                    inferred_content_type,
                    http_meta,
                    ResponseTransport::Full(jsonl),
                ));
                Ok(())
            }
            PipelineData::Value(value, meta) => {
                let http_meta = extract_http_response_meta(meta.as_ref());
                let _ = body_tx.send((
                    inferred_content_type,
                    http_meta,
                    ResponseTransport::Full(value_to_bytes(value)),
                ));
                Ok(())
            }
            PipelineData::ListStream(stream, meta) => {
                let http_meta = extract_http_response_meta(meta.as_ref());
                let (stream_tx, stream_rx) = tokio_mpsc::channel(32);
                let mut iter = stream.into_inner();

                // Peek first value to determine mode
                let first = iter.next();
                let use_jsonl = first.as_ref().is_some_and(is_jsonl_record);
                let content_type = if use_jsonl {
                    Some("application/x-ndjson".to_string())
                } else {
                    inferred_content_type
                };

                let _ = body_tx.send((
                    content_type,
                    http_meta,
                    ResponseTransport::Stream(stream_rx),
                ));

                // Helper to send a value
                let send_value = |stream_tx: &tokio_mpsc::Sender<Vec<u8>>, value: Value| -> bool {
                    let bytes = if use_jsonl {
                        let mut line =
                            serde_json::to_vec(&value_to_json(&value)).unwrap_or_default();
                        line.push(b'\n');
                        line
                    } else {
                        value_to_bytes(value)
                    };
                    stream_tx.blocking_send(bytes).is_ok()
                };

                // Process first value
                if let Some(value) = first {
                    if let Value::Error { error, .. } = &value {
                        let working_set = StateWorkingSet::new(&engine.state);
                        log_error(&format_cli_error(&working_set, error.as_ref(), None));
                        return Ok(());
                    }
                    if !send_value(&stream_tx, value) {
                        return Ok(());
                    }
                }

                // Process remaining values
                for value in iter {
                    if let Value::Error { error, .. } = &value {
                        let working_set = StateWorkingSet::new(&engine.state);
                        log_error(&format_cli_error(&working_set, error.as_ref(), None));
                        break;
                    }
                    if !send_value(&stream_tx, value) {
                        break;
                    }
                }
                Ok(())
            }
            PipelineData::ByteStream(stream, meta) => {
                let http_meta = extract_http_response_meta(meta.as_ref());
                let (stream_tx, stream_rx) = tokio_mpsc::channel(32);
                let content_type = meta
                    .as_ref()
                    .and_then(|m| m.content_type.clone())
                    .or_else(|| Some("application/octet-stream".to_string()));
                let _ = body_tx.send((
                    content_type,
                    http_meta,
                    ResponseTransport::Stream(stream_rx),
                ));
                let mut reader = stream
                    .reader()
                    .ok_or_else(|| "ByteStream has no reader".to_string())?;
                let mut buf = vec![0; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            if stream_tx.blocking_send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(err) => {
                            // Try to extract ShellError from the io::Error for proper formatting
                            use nu_protocol::shell_error::bridge::ShellErrorBridge;
                            if let Some(bridge) = err
                                .get_ref()
                                .and_then(|e| e.downcast_ref::<ShellErrorBridge>())
                            {
                                let working_set = StateWorkingSet::new(&engine.state);
                                log_error(&format_cli_error(&working_set, &bridge.0, None));
                                break; // Error already logged, just stop streaming
                            }
                            return Err(err.into());
                        }
                    }
                }
                Ok(())
            }
        }
    }

    // Create a thread job for this evaluation
    let (sender, _receiver) = mpsc::channel();
    let signals = engine.state.signals().clone();
    let job = ThreadJob::new(signals, Some("HTTP Request".to_string()), sender);

    // Add the job to the engine's job table
    let job_id = {
        let mut jobs = engine.state.jobs.lock().expect("jobs mutex poisoned");
        jobs.add_job(Job::Thread(job.clone()))
    };

    std::thread::spawn(move || -> Result<(), std::convert::Infallible> {
        let mut meta_tx_opt = Some(meta_tx);
        let mut body_tx_opt = Some(body_tx);

        // Wrap the evaluation in catch_unwind so that panics don't poison the
        // async runtime and we can still send a response back to the caller.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut local_engine = (*engine).clone();
            local_engine.state.current_job.background_thread_job = Some(job);

            // Take the senders for the inner call. If the evaluation completes
            // successfully, these senders will have been consumed. Otherwise we
            // will use the remaining ones to send an error response.
            inner(
                Arc::new(local_engine),
                request,
                stream,
                meta_tx_opt.take().unwrap(),
                body_tx_opt.take().unwrap(),
            )
        }));

        let err_msg: Option<String> = match result {
            Ok(Ok(())) => None,
            Ok(Err(e)) => Some(e.to_string()),
            Err(panic) => Some(format!("panic: {panic:?}")),
        };

        if let Some(err) = err_msg {
            log_error(&err);
            // Drop meta_tx - we don't use it for normal responses anymore
            // (only .static and .reverse-proxy use it)
            drop(meta_tx_opt.take());
            if let Some(body_tx) = body_tx_opt.take() {
                let error_meta = HttpResponseMeta {
                    status: Some(500),
                    headers: std::collections::HashMap::new(),
                };
                let _ = body_tx.send((
                    Some("text/plain; charset=utf-8".to_string()),
                    error_meta,
                    ResponseTransport::Full(format!("Script error: {err}").into_bytes()),
                ));
            }
        }

        // Clean up job when done
        {
            let mut jobs = engine.state.jobs.lock().expect("jobs mutex poisoned");
            jobs.remove_job(job_id);
        }

        Ok(())
    });

    (meta_rx, body_rx)
}
