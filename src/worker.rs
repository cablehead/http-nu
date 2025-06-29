use crate::commands::RESPONSE_TX;
use crate::request::{request_to_value, Request};
use crate::response::{value_to_bytes, Response, ResponseTransport};
use nu_protocol::{
    engine::{Job, ThreadJob},
    PipelineData, Value,
};
use std::io::Read;
use std::sync::{mpsc, Arc};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

pub fn spawn_eval_thread(
    engine: Arc<crate::Engine>,
    request: Request,
    stream: nu_protocol::ByteStream,
) -> (
    oneshot::Receiver<Response>,
    oneshot::Receiver<(Option<String>, ResponseTransport)>,
) {
    let (meta_tx, meta_rx) = tokio::sync::oneshot::channel();
    let (body_tx, body_rx) = tokio::sync::oneshot::channel();

    fn inner(
        engine: Arc<crate::Engine>,
        request: Request,
        stream: nu_protocol::ByteStream,
        meta_tx: oneshot::Sender<Response>,
        body_tx: oneshot::Sender<(Option<String>, ResponseTransport)>,
    ) -> Result<(), BoxError> {
        RESPONSE_TX.with(|tx| {
            *tx.borrow_mut() = Some(meta_tx);
        });
        let result = engine.eval(
            request_to_value(&request, nu_protocol::Span::unknown()),
            stream.into(),
        );
        // Always clear the thread local storage after eval completes
        RESPONSE_TX.with(|tx| {
            let _ = tx.borrow_mut().take(); // This will drop the sender if it wasn't used
        });
        let output = result?;
        let inferred_content_type = match &output {
            PipelineData::Value(Value::Record { .. }, meta)
                if meta.as_ref().and_then(|m| m.content_type.clone()).is_none() =>
            {
                Some("application/json".to_string())
            }
            PipelineData::Value(_, meta) | PipelineData::ListStream(_, meta) => {
                meta.as_ref().and_then(|m| m.content_type.clone())
            }
            _ => None,
        };
        match output {
            PipelineData::Empty => {
                let _ = body_tx.send((inferred_content_type, ResponseTransport::Empty));
                Ok(())
            }
            PipelineData::Value(Value::Nothing { .. }, _) => {
                let _ = body_tx.send((inferred_content_type, ResponseTransport::Empty));
                Ok(())
            }
            PipelineData::Value(value, _) => {
                let _ = body_tx.send((
                    inferred_content_type,
                    ResponseTransport::Full(value_to_bytes(value)),
                ));
                Ok(())
            }
            PipelineData::ListStream(stream, _) => {
                let (stream_tx, stream_rx) = tokio_mpsc::channel(32);
                let _ = body_tx.send((inferred_content_type, ResponseTransport::Stream(stream_rx)));
                for value in stream.into_inner() {
                    if stream_tx.blocking_send(value_to_bytes(value)).is_err() {
                        break;
                    }
                }
                Ok(())
            }
            PipelineData::ByteStream(stream, meta) => {
                let (stream_tx, stream_rx) = tokio_mpsc::channel(32);
                let _ = body_tx.send((
                    meta.as_ref().and_then(|m| m.content_type.clone()),
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
                        Err(err) => return Err(err.into()),
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
        let mut local_engine = (*engine).clone();
        local_engine.state.current_job.background_thread_job = Some(job);

        if let Err(e) = inner(Arc::new(local_engine), request, stream, meta_tx, body_tx) {
            eprintln!("Error in eval thread: {e}");
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
