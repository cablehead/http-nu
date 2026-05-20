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
use bytes::Bytes;
use nu_engine::command_prelude::*;
use nu_protocol::{
    record, shell_error::generic::GenericError, ByteStream, ByteStreamType, Category, PipelineData,
    ShellError, Signature, Span, SyntaxShape, Type, Value,
};
use portable_pty::{native_pty_system, Child as PortableChild, CommandBuilder, MasterPty, PtySize};

use crate::bus::Bus;

// --- session bookkeeping ----------------------------------------------------

const PTY_EVENTS_TOPIC: &str = "pty.events";

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    // Shared with the reader thread so it can inject auto-replies to DA1/
    // DA2/DSR queries (zmx-style) without round-tripping to the browser.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Box<dyn PortableChild + Send + Sync>,
    meta: HashMap<String, Value>,
    // Single-client output slot. The reader thread maintains a vt100::Parser
    // (canonical virtual screen) and, if a Sender is installed, forwards each
    // chunk to it. A new `pty stream` attach drops whatever Sender lives
    // here, which drains and closes the previous SSE's Receiver -- last
    // attach wins. The new attach gets a fresh state_formatted snapshot then
    // starts tailing the new channel.
    //
    // TODO: revisit input gating. POST /pty/input is a separate HTTP request
    // from /pty/stream, so a kicked tab can still POST keystrokes until its
    // EventSource error handler fires. Cheapest fix is a per-attach ticket:
    // /pty/stream rotates the pty's current ticket; /pty/input must present
    // a matching one. Not blocking for v1 -- worst case a stale tab types a
    // few chars that get applied to the same pty.
    output_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<Bytes>>>>,
    parser: Arc<Mutex<vt100::Parser>>,
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
        let parser = session.parser.clone();
        let writer = session.writer.clone();
        sessions().lock().unwrap().insert(sid.clone(), session);

        // Spawn the reader thread now (after insert) so it can self-reap
        // by sid when the child eventually exits.
        spawn_reader(
            sid.clone(),
            reader,
            output_tx,
            parser,
            writer,
            self.bus.clone(),
        );

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

    // The reader thread feeds the vt100 parser (canonical screen state) and,
    // if an attach has installed a Sender, forwards each chunk to it.
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| err(span, "clone_reader failed", e.to_string()))?;
    let output_tx = Arc::new(Mutex::new(None));
    let parser = Arc::new(Mutex::new(vt100::Parser::new(size.rows, size.cols, 0)));

    let session = PtySession {
        master: pair.master,
        writer: Arc::new(Mutex::new(writer)),
        child,
        meta: HashMap::new(),
        output_tx,
        parser,
    };
    Ok((session, reader))
}

/// Drain the pty master fd on a dedicated blocking thread. Each chunk is
/// fed to the vt100 parser (which updates the virtual screen state used by
/// future attachers) and, if a Sender is installed, forwarded to it. When
/// read returns 0 the child has exited; the thread removes the session from
/// the map (if `pty close` didn't already) and publishes `died` on the bus.
fn spawn_reader(
    sid: String,
    mut reader: Box<dyn Read + Send>,
    output_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<Bytes>>>>,
    parser: Arc<Mutex<vt100::Parser>>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    bus: Arc<Bus>,
) {
    let dump_path = std::env::var("HTTP_NU_PTY_DUMP").ok();
    std::thread::spawn(move || {
        let mut dump_file = dump_path.as_deref().and_then(|p| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .ok()
        });
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = Bytes::copy_from_slice(&buf[..n]);
                    if let Some(f) = dump_file.as_mut() {
                        let _ = writeln!(f, "[{sid}] {}", escape_bytes(&buf[..n]));
                        let _ = f.flush();
                    }
                    // Hold the parser lock across process+send so an attach
                    // racing with us either sees the snapshot before this
                    // chunk lands (and gets the chunk on its new channel) or
                    // sees the snapshot after (and skips this chunk on its
                    // new channel because the snapshot already reflects it).
                    let mut parser_guard = parser.lock().unwrap();
                    parser_guard.process(&buf[..n]);
                    // Forward raw bytes to the attached subscriber (if any).
                    // When a subscriber is attached the browser is itself a
                    // full VT emulator and natively auto-replies to all
                    // queries (DA1/DA2/DSR), so we stay out of its way.
                    // When no subscriber is attached we fall back to a
                    // zmx-style server-side reply for DA1/DA2 so the slave
                    // program (e.g. fish, reedline) doesn't block forever
                    // waiting for an answer that has nowhere to come from.
                    let has_subscriber = {
                        let tx_lock = output_tx.lock().unwrap();
                        if let Some(tx) = tx_lock.as_ref() {
                            let _ = tx.send(chunk);
                            true
                        } else {
                            false
                        }
                    };
                    if !has_subscriber {
                        let replies = scan_da_queries(&buf[..n]);
                        if !replies.is_empty() {
                            let mut w = writer.lock().unwrap();
                            for r in &replies {
                                let _ = w.write_all(r);
                            }
                            let _ = w.flush();
                        }
                    }
                    drop(parser_guard);
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

/// Render a byte slice with ANSI escapes / control chars made visible, so
/// HTTP_NU_PTY_DUMP=/tmp/pty.log dumps are readable in `tail`.
fn escape_bytes(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() * 2);
    for &c in b {
        match c {
            0x1b => s.push_str("\\e"),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            0x20..=0x7e => s.push(c as char),
            _ => s.push_str(&format!("\\x{c:02x}")),
        }
    }
    s
}

/// Scan a chunk of pty output for DA1/DA2 queries and return the canonical
/// replies to write to pty stdin. Matches zmx's `respondToDeviceAttributes`
/// (zmx/src/util.zig:149): only the device-attribute queries, same exact
/// reply bytes.
///
/// Caller is expected to invoke this only when no client is attached --
/// when a client IS attached, the browser's VT emulator handles all
/// queries natively, and a server-side reply would race/double-reply.
/// This is exactly zmx's gating policy (see its comment at util.zig:151:
/// "This handles the case where no client is attached").
///
/// Skips replies (CSI sequences with `?` after `[`) so we don't match
/// our own DA1 reply as a query.
///
/// Limitation: queries that straddle a chunk boundary are missed. In
/// practice programs emit these queries as a single short write, well
/// inside one 4KB chunk.
fn scan_da_queries(chunk: &[u8]) -> Vec<Vec<u8>> {
    let mut replies: Vec<Vec<u8>> = Vec::new();
    let mut i = 0;
    while i + 1 < chunk.len() {
        if chunk[i] != 0x1b || chunk[i + 1] != b'[' {
            i += 1;
            continue;
        }
        let mut j = i + 2;
        let private = if j < chunk.len() && matches!(chunk[j], b'<' | b'=' | b'>' | b'?') {
            let p = chunk[j];
            j += 1;
            Some(p)
        } else {
            None
        };
        let params_start = j;
        while j < chunk.len() && matches!(chunk[j], b'0'..=b'9' | b';') {
            j += 1;
        }
        if j >= chunk.len() {
            break;
        }
        let params = &chunk[params_start..j];
        let final_byte = chunk[j];
        match (private, final_byte) {
            (None, b'c') if params.is_empty() || params == b"0" => {
                replies.push(b"\x1b[?62;22c".to_vec());
            }
            (Some(b'>'), b'c') if params.is_empty() || params == b"0" => {
                replies.push(b"\x1b[>1;10;0c".to_vec());
            }
            _ => {}
        }
        i = j + 1;
    }
    replies
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

        let writer = {
            let map = sessions().lock().unwrap();
            let session = map
                .get(&sid)
                .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;
            session.writer.clone()
        };
        let mut w = writer.lock().unwrap();
        w.write_all(&bytes)
            .map_err(|e| err(head, "pty write failed", e.to_string()))?;
        let _ = w.flush();

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
            session
                .parser
                .lock()
                .unwrap()
                .screen_mut()
                .set_size(rows as u16, cols as u16);
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

        // Snapshot the parser's screen state and install a fresh Sender
        // atomically. Holding the parser lock across both keeps the reader
        // thread from processing new bytes during this window: any chunk
        // that arrives after we drop the lock either lands in our new
        // channel (good) or was already absorbed into the snapshot (also
        // good). Replacing the Sender drops whatever the previous attach
        // installed, which closes that consumer's stream -- last attach
        // wins. state_formatted emits the canonical ANSI sequence that
        // reproduces the current screen -- no stale DSR queries, no partial
        // sequences, no out-of-order mode toggles.
        let (rx, snapshot) = {
            let map = sessions().lock().unwrap();
            let session = map
                .get(&sid)
                .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;
            let parser_guard = session.parser.lock().unwrap();
            let snap = parser_guard.screen().state_formatted();
            let (tx, rx) = std::sync::mpsc::channel::<Bytes>();
            *session.output_tx.lock().unwrap() = Some(tx);
            drop(parser_guard);
            (rx, snap)
        };

        let ty = if sse {
            ByteStreamType::String
        } else {
            ByteStreamType::Binary
        };

        let mut snapshot_iter = if snapshot.is_empty() {
            None
        } else {
            Some(snapshot)
        };

        let stream = ByteStream::from_fn(
            head,
            engine_state.signals().clone(),
            ty,
            move |buffer: &mut Vec<u8>| {
                // First serve the snapshot of the current screen state.
                if let Some(bytes) = snapshot_iter.take() {
                    emit(buffer, &bytes, sse);
                    return Ok(true);
                }
                // Then tail the channel. Err means the Sender was replaced
                // (newer attach evicted us) or the session went away.
                match rx.recv() {
                    Ok(chunk) => {
                        emit(buffer, &chunk, sse);
                        Ok(true)
                    }
                    Err(_) => Ok(false),
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
