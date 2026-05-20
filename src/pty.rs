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

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex, OnceLock};

use base64::Engine as _;
use nu_engine::command_prelude::*;
use nu_protocol::{
    record, shell_error::generic::GenericError, ByteStream, ByteStreamType, Category, PipelineData,
    ShellError, Signature, Span, SyntaxShape, Type, Value,
};

use portable_pty::{native_pty_system, Child as PortableChild, CommandBuilder, MasterPty, PtySize};

use crate::bus::Bus;

// --- session bookkeeping ----------------------------------------------------

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    reader_taken: bool,
    child: Box<dyn PortableChild + Send + Sync>,
    meta: HashMap<String, Value>,
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
pub struct PtyOpenCommand;

impl PtyOpenCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PtyOpenCommand {
    fn default() -> Self {
        Self::new()
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

        let session = if embedded {
            open_embedded(engine_state, size, head)?
        } else {
            let cmd = cmd.ok_or_else(|| err(head, "missing cmd", "required without --embedded"))?;
            open_exec(&cmd, args, size, head)?
        };

        let sid = scru128::new().to_string();
        sessions().lock().unwrap().insert(sid.clone(), session);
        Ok(PipelineData::Value(Value::string(sid, head), None))
    }
}

#[allow(clippy::result_large_err)]
fn open_exec(
    cmd: &str,
    args: Option<Vec<String>>,
    size: PtySize,
    span: Span,
) -> Result<PtySession, ShellError> {
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

    Ok(PtySession {
        master: pair.master,
        writer,
        reader_taken: false,
        child,
        meta: HashMap::new(),
    })
}

#[allow(clippy::result_large_err)]
fn open_embedded(
    _engine_state: &EngineState,
    size: PtySize,
    span: Span,
) -> Result<PtySession, ShellError> {
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
pub struct PtyResizeCommand;

impl PtyResizeCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PtyResizeCommand {
    fn default() -> Self {
        Self::new()
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

        let mut reader = {
            let mut map = sessions().lock().unwrap();
            let session = map
                .get_mut(&sid)
                .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;
            if session.reader_taken {
                return Err(err(head, "pty stream already taken for this sid", ""));
            }
            session.reader_taken = true;
            session
                .master
                .try_clone_reader()
                .map_err(|e| err(head, "clone_reader failed", e.to_string()))?
        };

        let ty = if sse {
            ByteStreamType::String
        } else {
            ByteStreamType::Binary
        };

        let stream = ByteStream::from_fn(
            head,
            engine_state.signals().clone(),
            ty,
            move |buffer: &mut Vec<u8>| {
                let mut tmp = [0u8; 4096];
                match reader.read(&mut tmp) {
                    Ok(0) => Ok(false),
                    Ok(n) => {
                        if sse {
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&tmp[..n]);
                            buffer.extend_from_slice(b"data: ");
                            buffer.extend_from_slice(b64.as_bytes());
                            buffer.extend_from_slice(b"\n\n");
                        } else {
                            buffer.extend_from_slice(&tmp[..n]);
                        }
                        Ok(true)
                    }
                    Err(e) => Err(ShellError::Generic(GenericError::new(
                        "pty read failed",
                        e.to_string(),
                        head,
                    ))),
                }
            },
        );

        Ok(PipelineData::ByteStream(stream, None))
    }
}

// --- pty close --------------------------------------------------------------

#[derive(Clone)]
pub struct PtyCloseCommand;

impl PtyCloseCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PtyCloseCommand {
    fn default() -> Self {
        Self::new()
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
        let sid: String = call.req(engine_state, stack, 0)?;
        let session = sessions().lock().unwrap().remove(&sid);
        if let Some(mut s) = session {
            let _ = s.child.kill();
            let _ = s.child.wait();
        }
        Ok(PipelineData::Empty)
    }
}

// --- pty meta ---------------------------------------------------------------
//
// Free-form per-session metadata (label, etc). `pty meta set` mutates the
// session's meta map and pings the `pty.events` bus topic so any subscribed
// UI can react.

const PTY_EVENTS_TOPIC: &str = "pty.events";

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
