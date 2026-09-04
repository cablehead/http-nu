use crate::commands::RESPONSE_TX;
use crate::logging::log_error;
use crate::request::{request_to_value, Request};
use crate::response::{
    extract_http_response_meta, value_to_bytes, value_to_json, HttpResponseMeta, Response,
    ResponseTransport,
};
use nu_protocol::{
    engine::{Job, StateWorkingSet, ThreadJob},
    format_cli_error, PipelineData, Signals, Value,
};
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

/// Check if a value is a record without __html field
fn is_jsonl_record(value: &Value) -> bool {
    matches!(value, Value::Record { val, .. } if val.get("__html").is_none())
}

/// Kill this request's job once its response channel has no receiver left --
/// the client disconnected, or the response finished and hyper dropped the
/// body. Either way there is nothing left to protect; `job.kill()` after a
/// normal finish is a harmless no-op (the job is already done, nothing
/// downstream inspects `signals.interrupted()` after the fact).
///
/// `job.kill()` triggers this request's own `Signals` (see spawn_eval_thread:
/// each request gets its own, not a clone of the engine's process-wide one),
/// so a `.cat --follow`/`.last --follow` blocked in `Store::blocking_recv`
/// notices and returns instead of leaking its thread for the life of the
/// server. It also reaps any external-command PIDs this request's closure
/// spawned, via the same path Ctrl-C already used.
fn watch_disconnect(
    runtime: &tokio::runtime::Handle,
    tx: tokio_mpsc::Sender<Vec<u8>>,
    job: ThreadJob,
) {
    runtime.spawn(async move {
        tx.closed().await;
        let _ = job.kill();
    });
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
        runtime: tokio::runtime::Handle,
        job: ThreadJob,
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

        // Content-type inference (when pipeline metadata has no content-type):
        //
        // | Value type       | Content-Type           | Conversion          |
        // |------------------|------------------------|---------------------|
        // | Record (__html)  | text/html              | unwrap __html       |
        // | Record           | application/json       | JSON object         |
        // | List             | application/json       | JSON array          |
        // | Binary           | application/octet-stream | raw bytes         |
        // | Empty/Nothing    | None (no header)       | empty               |
        // | ListStream       | application/x-ndjson   | JSONL (if records)  |
        // | Other            | text/html (default)    | .to_string()        |
        //
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
            PipelineData::Value(Value::List { .. }, meta)
                if meta.as_ref().and_then(|m| m.content_type.clone()).is_none() =>
            {
                Some("application/json".to_string())
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
                Err(format_cli_error(None, &working_set, error.as_ref(), None).into())
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
                watch_disconnect(&runtime, stream_tx.clone(), job.clone());
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
                        log_error(&format_cli_error(None, &working_set, error.as_ref(), None));
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
                        log_error(&format_cli_error(None, &working_set, error.as_ref(), None));
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
                watch_disconnect(&runtime, stream_tx.clone(), job.clone());
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
                                log_error(&format_cli_error(None, &working_set, &bridge.0, None));
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

    // Each request gets its own interrupt flag, not a clone of the engine's
    // process-wide `engine.state.signals()`. With the process-wide one,
    // killing any single request's job -- including the per-connection
    // disconnect kill added below -- trips every other concurrent request's
    // signals too. Ctrl-C still stops everything: it kills every job in the
    // table (main.rs's setup_ctrlc_handler), one `ThreadJob::kill()` call
    // per job, each tripping its own request's flag.
    let interrupt = Arc::new(AtomicBool::new(false));
    let signals = Signals::new(interrupt);

    // Create a thread job for this evaluation
    let (sender, _receiver) = mpsc::channel();
    let job = ThreadJob::new(signals.clone(), Some("HTTP Request".to_string()), sender);

    // Add the job to the engine's job table
    let job_id = {
        let mut jobs = engine.state.jobs.lock().expect("jobs mutex poisoned");
        jobs.add_job(Job::Thread(job.clone()))
    };

    // Captured here, on the async caller's thread, so it's usable from
    // `inner`'s plain std::thread (which has no tokio context of its own) to
    // spawn the disconnect watcher below.
    let runtime = tokio::runtime::Handle::current();

    std::thread::spawn(move || -> Result<(), std::convert::Infallible> {
        let mut meta_tx_opt = Some(meta_tx);
        let mut body_tx_opt = Some(body_tx);

        // Wrap the evaluation in catch_unwind so that panics don't poison the
        // async runtime and we can still send a response back to the caller.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut local_engine = (*engine).clone();
            local_engine.state.current_job.background_thread_job = Some(job.clone());
            // Route this request's own Signals to the engine the closure
            // actually runs on, not just the ThreadJob in the jobs table --
            // .cat/.last and any other signal-aware command check
            // engine_state.signals(), not the job table.
            local_engine.state.set_signals(signals.clone());

            // Take the senders for the inner call. If the evaluation completes
            // successfully, these senders will have been consumed. Otherwise we
            // will use the remaining ones to send an error response.
            inner(
                Arc::new(local_engine),
                request,
                stream,
                meta_tx_opt.take().unwrap(),
                body_tx_opt.take().unwrap(),
                runtime,
                job,
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
