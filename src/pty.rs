//! `pty` commands for http-nu: open/write/resize/stream/close.
//!
//! Sessions live in a process-wide map keyed by sid. Output is exposed as a
//! ByteStream so SSE handlers can pipe it straight to the response.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use nu_engine::command_prelude::*;
use nu_protocol::{
    shell_error::generic::GenericError, ByteStream, ByteStreamType, Category, PipelineData,
    ShellError, Signature, Span, SyntaxShape, Type, Value,
};

use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    reader_taken: bool,
    child: Box<dyn Child + Send + Sync>,
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
        "Open a pseudo-terminal session running cmd; returns the sid"
    }

    fn signature(&self) -> Signature {
        Signature::build("pty open")
            .required("cmd", SyntaxShape::String, "command to spawn")
            .named(
                "args",
                SyntaxShape::List(Box::new(SyntaxShape::String)),
                "command arguments",
                None,
            )
            .named("cols", SyntaxShape::Int, "initial columns (default 80)", None)
            .named("rows", SyntaxShape::Int, "initial rows (default 24)", None)
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
        let cmd: String = call.req(engine_state, stack, 0)?;
        let args: Option<Vec<String>> = call.get_flag(engine_state, stack, "args")?;
        let cols: Option<i64> = call.get_flag(engine_state, stack, "cols")?;
        let rows: Option<i64> = call.get_flag(engine_state, stack, "rows")?;

        let size = PtySize {
            cols: cols.unwrap_or(80) as u16,
            rows: rows.unwrap_or(24) as u16,
            pixel_width: 0,
            pixel_height: 0,
        };

        let pair = native_pty_system()
            .openpty(size)
            .map_err(|e| err(head, "openpty failed", e.to_string()))?;

        let mut builder = CommandBuilder::new(&cmd);
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
            .map_err(|e| err(head, "spawn failed", e.to_string()))?;
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| err(head, "take_writer failed", e.to_string()))?;

        let sid = scru128::new().to_string();
        sessions().lock().unwrap().insert(
            sid.clone(),
            PtySession {
                master: pair.master,
                writer,
                reader_taken: false,
                child,
            },
        );

        Ok(PipelineData::Value(Value::string(sid, head), None))
    }
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
                            let b64 =
                                base64::engine::general_purpose::STANDARD.encode(&tmp[..n]);
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
