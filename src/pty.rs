//! `pty` commands for http-nu: open/write/resize/stream/close.
//!
//! Two backends:
//! - exec: fork+exec an external command via portable-pty
//! - embedded: fork the http-nu process, run nu's REPL in the child against
//!   a clone of the current EngineState. No external `nu` binary needed;
//!   the in-browser REPL has access to http-nu's custom commands.
//!
//! Sessions live in a process-wide map keyed by sid. Output is exposed as a
//! ByteStream so SSE handlers can pipe it straight to the response.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use bytes::Bytes;
use nu_engine::command_prelude::*;
use nu_protocol::{
    record, shell_error::generic::GenericError, ByteStream, ByteStreamType, Category, PipelineData,
    ShellError, Signature, Span, SyntaxShape, Type, Value,
};
use tokio::sync::broadcast;

use portable_pty::{native_pty_system, Child as PortableChild, CommandBuilder, MasterPty, PtySize};

use crate::bus::Bus;

// --- session bookkeeping ----------------------------------------------------

const BACKLOG_BYTES: usize = 64 * 1024;
const BROADCAST_CAPACITY: usize = 256;
const PTY_EVENTS_TOPIC: &str = "pty.events";

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn PortableChild + Send + Sync>,
    meta: HashMap<String, Value>,
    // One dedicated reader thread per session fans bytes from the master fd
    // into this broadcast channel. Each `pty stream` call subscribes; on
    // attach it first replays `backlog`, then tails new bytes from the
    // subscription. Allows N concurrent consumers and reattach after the
    // browser disconnects.
    output_tx: broadcast::Sender<Bytes>,
    backlog: Arc<Mutex<VecDeque<u8>>>,
}

fn sessions() -> &'static Mutex<HashMap<String, PtySession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, PtySession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[allow(clippy::result_large_err)]
fn err(span: Span, msg: impl Into<String>, label: impl Into<String>) -> ShellError {
    ShellError::Generic(GenericError::new(msg.into(), label.into(), span))
}

// --- pty open ---------------------------------------------------------------

#[derive(Clone)]
pub struct PtyOpenCommand {
    bus: Arc<Bus>,
}

impl PtyOpenCommand {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self { bus }
    }
}

impl Command for PtyOpenCommand {
    fn name(&self) -> &str {
        "pty open"
    }

    fn description(&self) -> &str {
        "Open a pseudo-terminal session and return its sid. With --embedded, fork http-nu and run nu's REPL in the child instead of execing an external command."
    }

    fn signature(&self) -> Signature {
        Signature::build("pty open")
            .optional(
                "cmd",
                SyntaxShape::String,
                "command to spawn (ignored with --embedded)",
            )
            .named(
                "args",
                SyntaxShape::List(Box::new(SyntaxShape::String)),
                "command arguments",
                None,
            )
            .named(
                "cols",
                SyntaxShape::Int,
                "initial columns (default 80)",
                None,
            )
            .named("rows", SyntaxShape::Int, "initial rows (default 24)", None)
            .switch(
                "embedded",
                "fork http-nu and run nu's REPL in-process (no external nu binary)",
                None,
            )
            .input_output_types(vec![(Type::Nothing, Type::String)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let cmd: Option<String> = call.opt(engine_state, stack, 0)?;
        let args: Option<Vec<String>> = call.get_flag(engine_state, stack, "args")?;
        let cols: Option<i64> = call.get_flag(engine_state, stack, "cols")?;
        let rows: Option<i64> = call.get_flag(engine_state, stack, "rows")?;
        let embedded = call.has_flag(engine_state, stack, "embedded")?;

        let size = PtySize {
            cols: cols.unwrap_or(80) as u16,
            rows: rows.unwrap_or(24) as u16,
            pixel_width: 0,
            pixel_height: 0,
        };

        let (session, reader) = if embedded {
            open_embedded(engine_state, size, head)?
        } else {
            let cmd = cmd.ok_or_else(|| err(head, "missing cmd", "required without --embedded"))?;
            open_exec(&cmd, args, size, head)?
        };

        let sid = scru128::new().to_string();
        let (cols_n, rows_n) = (size.cols as i64, size.rows as i64);
        let output_tx = session.output_tx.clone();
        let backlog = session.backlog.clone();
        sessions().lock().unwrap().insert(sid.clone(), session);

        // Spawn the reader thread now (after insert) so it can self-reap
        // by sid when the child eventually exits.
        spawn_reader(sid.clone(), reader, output_tx, backlog, self.bus.clone());

        self.bus.publish(
            PTY_EVENTS_TOPIC,
            Value::record(
                record! {
                    "event" => Value::string("created", head),
                    "sid" => Value::string(&sid, head),
                    "cols" => Value::int(cols_n, head),
                    "rows" => Value::int(rows_n, head),
                },
                head,
            ),
        );

        Ok(PipelineData::Value(Value::string(sid, head), None))
    }
}

#[allow(clippy::result_large_err)]
fn open_exec(
    cmd: &str,
    args: Option<Vec<String>>,
    size: PtySize,
    span: Span,
) -> Result<(PtySession, Box<dyn Read + Send>), ShellError> {
    let pair = native_pty_system()
        .openpty(size)
        .map_err(|e| err(span, "openpty failed", e.to_string()))?;

    let mut builder = CommandBuilder::new(cmd);
    if let Some(args) = args {
        for a in args {
            builder.arg(a);
        }
    }
    for (k, v) in std::env::vars() {
        builder.env(k, v);
    }
    builder.env("TERM", "xterm-256color");
    if let Ok(cwd) = std::env::current_dir() {
        builder.cwd(cwd);
    }

    let child = pair
        .slave
        .spawn_command(builder)
        .map_err(|e| err(span, "spawn failed", e.to_string()))?;
    drop(pair.slave);

    let writer = pair
        .master
        .take_writer()
        .map_err(|e| err(span, "take_writer failed", e.to_string()))?;

    // Spawn the dedicated reader thread that pulls from the master fd and
    // fans bytes out via a broadcast channel + backlog ring. When read
    // returns 0 the child has exited; the thread also reaps the session
    // (if we beat `pty close` to it) and publishes a `died` event.
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| err(span, "clone_reader failed", e.to_string()))?;
    let (output_tx, _) = broadcast::channel::<Bytes>(BROADCAST_CAPACITY);
    let backlog = Arc::new(Mutex::new(VecDeque::with_capacity(BACKLOG_BYTES)));

    let session = PtySession {
        master: pair.master,
        writer,
        child,
        meta: HashMap::new(),
        output_tx,
        backlog,
    };
    Ok((session, reader))
}

/// Drain the pty master fd on a dedicated blocking thread. Each chunk goes
/// into the backlog ring (trimmed to BACKLOG_BYTES) and is broadcast to any
/// `pty stream` subscribers. When read returns 0 the child has exited; the
/// thread removes the session from the map (if `pty close` didn't already)
/// and publishes a `died` event on the bus.
fn spawn_reader(
    sid: String,
    mut reader: Box<dyn Read + Send>,
    tx: broadcast::Sender<Bytes>,
    backlog: Arc<Mutex<VecDeque<u8>>>,
    bus: Arc<Bus>,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = Bytes::copy_from_slice(&buf[..n]);
                    {
                        let mut bl = backlog.lock().unwrap();
                        bl.extend(&buf[..n]);
                        while bl.len() > BACKLOG_BYTES {
                            bl.pop_front();
                        }
                    }
                    let _ = tx.send(chunk);
                }
                Err(e) => {
                    use std::io::ErrorKind;
                    if matches!(e.kind(), ErrorKind::Interrupted) {
                        continue;
                    }
                    break;
                }
            }
        }
        // EOF on master = child exited. If the session is still in the map,
        // we beat `pty close` to it: reap, then publish `died`. If the entry
        // is already gone, the close command handled cleanup + published
        // `deleted`, so don't publish anything here.
        let removed = sessions().lock().unwrap().remove(&sid);
        if let Some(mut s) = removed {
            let code = match s.child.wait() {
                Ok(es) => es.exit_code() as i64,
                Err(_) => -1,
            };
            let span = Span::unknown();
            bus.publish(
                PTY_EVENTS_TOPIC,
                Value::record(
                    record! {
                        "event" => Value::string("died", span),
                        "sid" => Value::string(sid, span),
                        "code" => Value::int(code, span),
                    },
                    span,
                ),
            );
        }
    });
}

#[allow(clippy::result_large_err)]
fn open_embedded(
    _engine_state: &EngineState,
    size: PtySize,
    span: Span,
) -> Result<(PtySession, Box<dyn Read + Send>), ShellError> {
    // Self-re-exec into our own `repl` subcommand. The fork-no-exec variant
    // ran fine for trivial use but silently dropped output from bare
    // externals (e.g. `^ls`) due to interactions between http-nu's
    // multi-threaded Rust runtime state and nushell's foreground-job
    // setpgid+tcsetpgrp dance. Exec'ing our own binary gives the embedded
    // REPL a clean process state, with all of http-nu's custom commands
    // still registered (because `repl` rebuilds them in its main).
    let self_exe = std::env::current_exe()
        .map_err(|e| err(span, "current_exe failed", e.to_string()))?
        .into_os_string()
        .into_string()
        .map_err(|_| err(span, "current_exe path not utf-8", ""))?;
    open_exec(&self_exe, Some(vec!["repl".to_string()]), size, span)
}

// --- pty write --------------------------------------------------------------

#[derive(Clone)]
pub struct PtyWriteCommand;

impl PtyWriteCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PtyWriteCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for PtyWriteCommand {
    fn name(&self) -> &str {
        "pty write"
    }

    fn description(&self) -> &str {
        "Write piped bytes/string to the pty session's stdin"
    }

    fn signature(&self) -> Signature {
        Signature::build("pty write")
            .required("sid", SyntaxShape::String, "session id")
            .input_output_types(vec![
                (Type::Binary, Type::Nothing),
                (Type::String, Type::Nothing),
                (Type::Nothing, Type::Nothing),
            ])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let sid: String = call.req(engine_state, stack, 0)?;

        let bytes: Vec<u8> = match input {
            PipelineData::Empty => Vec::new(),
            PipelineData::Value(v, _) => crate::response::value_to_bytes(v),
            PipelineData::ByteStream(stream, _) => stream
                .into_bytes()
                .map_err(|e| err(head, "read stream", e.to_string()))?,
            PipelineData::ListStream(_, _) => {
                return Err(err(head, "list stream input not supported", ""));
            }
        };

        if bytes.is_empty() {
            return Ok(PipelineData::Empty);
        }

        let mut map = sessions().lock().unwrap();
        let session = map
            .get_mut(&sid)
            .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;
        session
            .writer
            .write_all(&bytes)
            .map_err(|e| err(head, "pty write failed", e.to_string()))?;
        let _ = session.writer.flush();

        Ok(PipelineData::Empty)
    }
}

// --- pty resize -------------------------------------------------------------

#[derive(Clone)]
pub struct PtyResizeCommand {
    bus: Arc<Bus>,
}

impl PtyResizeCommand {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self { bus }
    }
}

impl Command for PtyResizeCommand {
    fn name(&self) -> &str {
        "pty resize"
    }

    fn description(&self) -> &str {
        "Resize the pty (TIOCSWINSZ + SIGWINCH to the foreground group)"
    }

    fn signature(&self) -> Signature {
        Signature::build("pty resize")
            .required("sid", SyntaxShape::String, "session id")
            .required("cols", SyntaxShape::Int, "columns")
            .required("rows", SyntaxShape::Int, "rows")
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let sid: String = call.req(engine_state, stack, 0)?;
        let cols: i64 = call.req(engine_state, stack, 1)?;
        let rows: i64 = call.req(engine_state, stack, 2)?;

        {
            let map = sessions().lock().unwrap();
            let session = map
                .get(&sid)
                .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;
            session
                .master
                .resize(PtySize {
                    cols: cols as u16,
                    rows: rows as u16,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| err(head, "pty resize failed", e.to_string()))?;
        }

        self.bus.publish(
            PTY_EVENTS_TOPIC,
            Value::record(
                record! {
                    "event" => Value::string("resized", head),
                    "sid" => Value::string(sid, head),
                    "cols" => Value::int(cols, head),
                    "rows" => Value::int(rows, head),
                },
                head,
            ),
        );

        Ok(PipelineData::Empty)
    }
}

// --- pty stream -------------------------------------------------------------

#[derive(Clone)]
pub struct PtyStreamCommand;

impl PtyStreamCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PtyStreamCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for PtyStreamCommand {
    fn name(&self) -> &str {
        "pty stream"
    }

    fn description(&self) -> &str {
        "Stream pty output bytes; --sse wraps each chunk as a base64 SSE event"
    }

    fn signature(&self) -> Signature {
        Signature::build("pty stream")
            .required("sid", SyntaxShape::String, "session id")
            .switch("sse", "format as text/event-stream with base64 data", None)
            .input_output_types(vec![(Type::Nothing, Type::Binary)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let sid: String = call.req(engine_state, stack, 0)?;
        let sse = call.has_flag(engine_state, stack, "sse")?;

        // Subscribe + snapshot the backlog atomically so we don't drop bytes
        // that arrived between snapshot and subscribe.
        let (mut rx, backlog) = {
            let map = sessions().lock().unwrap();
            let session = map
                .get(&sid)
                .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;
            let rx = session.output_tx.subscribe();
            let snapshot: Vec<u8> = session.backlog.lock().unwrap().iter().copied().collect();
            (rx, snapshot)
        };

        let ty = if sse {
            ByteStreamType::String
        } else {
            ByteStreamType::Binary
        };

        let mut backlog_iter = if backlog.is_empty() {
            None
        } else {
            Some(backlog)
        };

        let stream = ByteStream::from_fn(
            head,
            engine_state.signals().clone(),
            ty,
            move |buffer: &mut Vec<u8>| {
                // First serve the snapshot of pre-attach bytes.
                if let Some(bytes) = backlog_iter.take() {
                    emit(buffer, &bytes, sse);
                    return Ok(true);
                }
                // Then tail the broadcast. Lagged consumers skip ahead; that's
                // fine for a terminal stream.
                loop {
                    match rx.blocking_recv() {
                        Ok(chunk) => {
                            emit(buffer, &chunk, sse);
                            return Ok(true);
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => return Ok(false),
                    }
                }
            },
        );

        Ok(PipelineData::ByteStream(stream, None))
    }
}

fn emit(buffer: &mut Vec<u8>, bytes: &[u8], sse: bool) {
    if sse {
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        buffer.extend_from_slice(b"data: ");
        buffer.extend_from_slice(b64.as_bytes());
        buffer.extend_from_slice(b"\n\n");
    } else {
        buffer.extend_from_slice(bytes);
    }
}

// --- pty close --------------------------------------------------------------

#[derive(Clone)]
pub struct PtyCloseCommand {
    bus: Arc<Bus>,
}

impl PtyCloseCommand {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self { bus }
    }
}

impl Command for PtyCloseCommand {
    fn name(&self) -> &str {
        "pty close"
    }

    fn description(&self) -> &str {
        "Close a pty session: kill child, drop fds"
    }

    fn signature(&self) -> Signature {
        Signature::build("pty close")
            .required("sid", SyntaxShape::String, "session id")
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let sid: String = call.req(engine_state, stack, 0)?;
        let session = sessions().lock().unwrap().remove(&sid);
        if let Some(mut s) = session {
            let _ = s.child.kill();
            let _ = s.child.wait();
            self.bus.publish(
                PTY_EVENTS_TOPIC,
                Value::record(
                    record! {
                        "event" => Value::string("deleted", head),
                        "sid" => Value::string(sid, head),
                    },
                    head,
                ),
            );
        }
        Ok(PipelineData::Empty)
    }
}

// --- pty meta ---------------------------------------------------------------
//
// Free-form per-session metadata (label, etc). `pty meta set` mutates the
// session's meta map and pings the `pty.events` bus topic so any subscribed
// UI can react.

#[derive(Clone)]
pub struct PtyMetaSetCommand {
    bus: Arc<Bus>,
}

impl PtyMetaSetCommand {
    pub fn new(bus: Arc<Bus>) -> Self {
        Self { bus }
    }
}

impl Command for PtyMetaSetCommand {
    fn name(&self) -> &str {
        "pty meta set"
    }

    fn description(&self) -> &str {
        "Set a free-form meta value on a pty session; publishes a ping on pty.events"
    }

    fn signature(&self) -> Signature {
        Signature::build("pty meta set")
            .required("sid", SyntaxShape::String, "session id")
            .required("key", SyntaxShape::String, "meta key")
            .required("value", SyntaxShape::Any, "meta value")
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let sid: String = call.req(engine_state, stack, 0)?;
        let key: String = call.req(engine_state, stack, 1)?;
        let value: Value = call.req(engine_state, stack, 2)?;

        {
            let mut map = sessions().lock().unwrap();
            let session = map
                .get_mut(&sid)
                .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;
            session.meta.insert(key.clone(), value.clone());
        }

        let event = Value::record(
            record! {
                "event" => Value::string("meta", head),
                "sid" => Value::string(sid, head),
                "key" => Value::string(key, head),
                "value" => value,
            },
            head,
        );
        self.bus.publish(PTY_EVENTS_TOPIC, event);

        Ok(PipelineData::Empty)
    }
}

// --- pty list ---------------------------------------------------------------

#[derive(Clone)]
pub struct PtyListCommand;

impl PtyListCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PtyListCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for PtyListCommand {
    fn name(&self) -> &str {
        "pty list"
    }

    fn description(&self) -> &str {
        "List all live pty sessions as [{sid, cols, rows, meta}, ...]"
    }

    fn signature(&self) -> Signature {
        Signature::build("pty list")
            .input_output_types(vec![(Type::Nothing, Type::Any)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        _engine_state: &EngineState,
        _stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let map = sessions().lock().unwrap();
        let mut rows: Vec<Value> = Vec::with_capacity(map.len());
        for (sid, s) in map.iter() {
            let size = s.master.get_size().ok();
            let cols = size.as_ref().map(|sz| sz.cols as i64).unwrap_or(0);
            let rs = size.as_ref().map(|sz| sz.rows as i64).unwrap_or(0);
            let mut meta_rec = nu_protocol::Record::new();
            for (k, v) in &s.meta {
                meta_rec.push(k.clone(), v.clone());
            }
            rows.push(Value::record(
                record! {
                    "sid" => Value::string(sid, head),
                    "cols" => Value::int(cols, head),
                    "rows" => Value::int(rs, head),
                    "meta" => Value::record(meta_rec, head),
                },
                head,
            ));
        }
        Ok(Value::list(rows, head).into_pipeline_data())
    }
}

#[derive(Clone)]
pub struct PtyMetaGetCommand;

impl PtyMetaGetCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PtyMetaGetCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for PtyMetaGetCommand {
    fn name(&self) -> &str {
        "pty meta get"
    }

    fn description(&self) -> &str {
        "Read pty session meta. With no key, returns the whole record; with a key, returns its value (or nothing)."
    }

    fn signature(&self) -> Signature {
        Signature::build("pty meta get")
            .required("sid", SyntaxShape::String, "session id")
            .optional(
                "key",
                SyntaxShape::String,
                "specific key; omit for whole record",
            )
            .input_output_types(vec![(Type::Nothing, Type::Any)])
            .category(Category::Custom("http".into()))
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let sid: String = call.req(engine_state, stack, 0)?;
        let key: Option<String> = call.opt(engine_state, stack, 1)?;

        let map = sessions().lock().unwrap();
        let session = map
            .get(&sid)
            .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;

        let out = match key {
            Some(k) => session
                .meta
                .get(&k)
                .cloned()
                .unwrap_or(Value::nothing(head)),
            None => {
                let mut rec = nu_protocol::Record::new();
                for (k, v) in &session.meta {
                    rec.push(k.clone(), v.clone());
                }
                Value::record(rec, head)
            }
        };
        Ok(out.into_pipeline_data())
    }
}
