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
use wezterm_term::{
    color::{ColorAttribute, ColorPalette},
    CellAttributes, Intensity, Terminal, TerminalConfiguration, TerminalSize, Underline,
};

use crate::bus::Bus;

// --- wezterm-term plumbing --------------------------------------------------

/// Minimal TerminalConfiguration impl. Default trait methods cover everything
/// except color_palette, which is required.
#[derive(Debug, Default)]
struct MinimalConfig;

impl TerminalConfiguration for MinimalConfig {
    fn color_palette(&self) -> ColorPalette {
        ColorPalette::default()
    }
}

/// Writer wrapper that delegates through the same Arc<Mutex<...>> the rest of
/// the session uses, AND suppresses writes when a client subscriber is
/// attached. The browser-side VT emulator natively replies to DA/DSR queries,
/// so when a browser is attached we let it answer; when nobody is attached we
/// let wezterm-term's auto-reply take over (zmx-style). This preserves the
/// existing no-double-reply policy.
struct ConditionalWriter {
    inner: Arc<Mutex<Box<dyn Write + Send>>>,
    has_subscriber: Arc<Mutex<bool>>,
}

impl Write for ConditionalWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if *self.has_subscriber.lock().unwrap() {
            return Ok(buf.len());
        }
        self.inner.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if *self.has_subscriber.lock().unwrap() {
            return Ok(());
        }
        self.inner.lock().unwrap().flush()
    }
}

/// Append the foreground SGR codes for `color` into `out` (without the
/// leading "\x1b[" or trailing "m"). Returns true if anything was written.
fn append_fg_sgr(out: &mut String, color: ColorAttribute) -> bool {
    use std::fmt::Write as _;
    match color {
        ColorAttribute::Default => false,
        ColorAttribute::PaletteIndex(i) if i < 8 => {
            let _ = write!(out, ";{}", 30 + i);
            true
        }
        ColorAttribute::PaletteIndex(i) if i < 16 => {
            let _ = write!(out, ";{}", 90 + (i - 8));
            true
        }
        ColorAttribute::PaletteIndex(i) => {
            let _ = write!(out, ";38;5;{i}");
            true
        }
        ColorAttribute::TrueColorWithDefaultFallback(rgb)
        | ColorAttribute::TrueColorWithPaletteFallback(rgb, _) => {
            let r = (rgb.0 * 255.0).round() as u8;
            let g = (rgb.1 * 255.0).round() as u8;
            let b = (rgb.2 * 255.0).round() as u8;
            let _ = write!(out, ";38;2;{r};{g};{b}");
            true
        }
    }
}

/// Append the background SGR codes for `color` into `out`. Same shape as
/// `append_fg_sgr` but with bg codes (40s/100s/48).
fn append_bg_sgr(out: &mut String, color: ColorAttribute) -> bool {
    use std::fmt::Write as _;
    match color {
        ColorAttribute::Default => false,
        ColorAttribute::PaletteIndex(i) if i < 8 => {
            let _ = write!(out, ";{}", 40 + i);
            true
        }
        ColorAttribute::PaletteIndex(i) if i < 16 => {
            let _ = write!(out, ";{}", 100 + (i - 8));
            true
        }
        ColorAttribute::PaletteIndex(i) => {
            let _ = write!(out, ";48;5;{i}");
            true
        }
        ColorAttribute::TrueColorWithDefaultFallback(rgb)
        | ColorAttribute::TrueColorWithPaletteFallback(rgb, _) => {
            let r = (rgb.0 * 255.0).round() as u8;
            let g = (rgb.1 * 255.0).round() as u8;
            let b = (rgb.2 * 255.0).round() as u8;
            let _ = write!(out, ";48;2;{r};{g};{b}");
            true
        }
    }
}

/// Emit a full SGR escape sequence describing the cell's attributes. Always
/// begins with reset (0) so caller doesn't have to track previous state.
fn append_sgr_full(out: &mut String, attrs: &CellAttributes) {
    out.push_str("\x1b[0");
    match attrs.intensity() {
        Intensity::Bold => out.push_str(";1"),
        Intensity::Half => out.push_str(";2"),
        Intensity::Normal => {}
    }
    if attrs.italic() {
        out.push_str(";3");
    }
    match attrs.underline() {
        Underline::None => {}
        Underline::Single => out.push_str(";4"),
        Underline::Double => out.push_str(";21"),
        // Curly/Dotted/Dashed degrade to single -- most clients still render
        // it as some form of underline.
        _ => out.push_str(";4"),
    }
    if attrs.reverse() {
        out.push_str(";7");
    }
    if attrs.invisible() {
        out.push_str(";8");
    }
    if attrs.strikethrough() {
        out.push_str(";9");
    }
    append_fg_sgr(out, attrs.foreground());
    append_bg_sgr(out, attrs.background());
    out.push('m');
}

/// Cheap structural equality on the attribute bits we render. CellAttributes
/// implements PartialEq, which compares all bits (including hyperlinks and
/// image refs we don't care about), so use a narrower check.
fn attrs_equiv(a: &CellAttributes, b: &CellAttributes) -> bool {
    a.attribute_bits_equal(b)
        && a.foreground() == b.foreground()
        && a.background() == b.background()
}

/// Build a VT byte sequence that, when written to a fresh xterm-compatible
/// terminal, reproduces the current visible screen + cursor + SGR state of
/// `term`.
fn snapshot_terminal(term: &Terminal) -> Vec<u8> {
    use std::fmt::Write as _;

    let size = term.get_size();
    let cols = size.cols;
    let cursor = term.cursor_pos();
    let screen = term.screen();
    let physical_rows = screen.physical_rows;
    let total_lines = screen.scrollback_rows();
    let start = total_lines.saturating_sub(physical_rows);
    let lines = screen.lines_in_phys_range(start..total_lines);

    let mut out = String::new();
    out.push_str("\x1b[0m\x1b[2J\x1b[H");

    // Track the attribute set we last emitted so we only emit SGR on change.
    let default_attrs = CellAttributes::default();
    let mut current = default_attrs.clone();

    for (row, line) in lines.iter().enumerate() {
        if row > 0 {
            // Reset before CRLF so trailing background fill doesn't leak
            // to the next line.
            if !attrs_equiv(&current, &default_attrs) {
                out.push_str("\x1b[0m");
                current = default_attrs.clone();
            }
            out.push_str("\r\n");
        }
        let mut col = 0usize;
        for cell_ref in line.visible_cells() {
            let cell_col = cell_ref.cell_index();
            while col < cell_col && col < cols {
                if !attrs_equiv(&current, &default_attrs) {
                    out.push_str("\x1b[0m");
                    current = default_attrs.clone();
                }
                out.push(' ');
                col += 1;
            }
            let cell_attrs = cell_ref.attrs();
            if !attrs_equiv(&current, cell_attrs) {
                append_sgr_full(&mut out, cell_attrs);
                current = cell_attrs.clone();
            }
            out.push_str(cell_ref.str());
            col += cell_ref.width().max(1);
            if col >= cols {
                break;
            }
        }
        while col < cols {
            if !attrs_equiv(&current, &default_attrs) {
                out.push_str("\x1b[0m");
                current = default_attrs.clone();
            }
            out.push(' ');
            col += 1;
        }
    }

    out.push_str("\x1b[0m");
    let _ = write!(
        out,
        "\x1b[{};{}H",
        (cursor.y as usize) + 1,
        cursor.x + 1
    );

    out.into_bytes()
}

// --- session bookkeeping ----------------------------------------------------

const PTY_EVENTS_TOPIC: &str = "pty.events";

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    // Shared with wezterm-term's Terminal (which uses it via ConditionalWriter
    // to auto-reply to DA1/DA2/DSR queries when no client is attached) and
    // with `pty write` (for user input). All three contend on the same Mutex,
    // which is fine -- this is a low-throughput interactive path.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Box<dyn PortableChild + Send + Sync>,
    meta: HashMap<String, Value>,
    // Single-client output slot. The reader thread feeds bytes into the
    // wezterm-term Terminal (which updates the canonical virtual screen state
    // and emits auto-replies via its writer) and, if a Sender is installed,
    // forwards each chunk to it. A new `pty stream` attach drops whatever
    // Sender lives here, which drains and closes the previous SSE's Receiver
    // -- last attach wins. The new attach gets a fresh screen snapshot then
    // starts tailing the new channel.
    //
    // TODO: revisit input gating. POST /pty/input is a separate HTTP request
    // from /pty/stream, so a kicked tab can still POST keystrokes until its
    // EventSource error handler fires. Cheapest fix is a per-attach ticket:
    // /pty/stream rotates the pty's current ticket; /pty/input must present
    // a matching one. Not blocking for v1 -- worst case a stale tab types a
    // few chars that get applied to the same pty.
    output_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<Bytes>>>>,
    term: Arc<Mutex<Terminal>>,
    // Flipped to `true` by `pty stream` when a subscriber attaches, `false`
    // when the reader thread notices the channel is closed. Read by
    // ConditionalWriter on the auto-reply path.
    has_subscriber: Arc<Mutex<bool>>,
}

fn sessions() -> &'static Mutex<HashMap<String, PtySession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, PtySession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Hand out the master writer for a live session. Used by the handler-layer
/// fast-path for POST /pty/input so per-keystroke requests skip the nushell
/// eval thread entirely. The Arc shares the same Mutex as `pty write` and
/// the wezterm-term auto-reply path, so writes from all three serialize.
pub(crate) fn writer_for(sid: &str) -> Option<Arc<Mutex<Box<dyn Write + Send>>>> {
    sessions()
        .lock()
        .unwrap()
        .get(sid)
        .map(|s| s.writer.clone())
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
        let term = session.term.clone();
        let has_subscriber = session.has_subscriber.clone();
        sessions().lock().unwrap().insert(sid.clone(), session);

        // Spawn the reader thread now (after insert) so it can self-reap
        // by sid when the child eventually exits.
        spawn_reader(
            sid.clone(),
            reader,
            output_tx,
            term,
            has_subscriber,
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

    let raw_writer = pair
        .master
        .take_writer()
        .map_err(|e| err(span, "take_writer failed", e.to_string()))?;

    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|e| err(span, "clone_reader failed", e.to_string()))?;

    let output_tx = Arc::new(Mutex::new(None));
    let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(raw_writer));
    let has_subscriber = Arc::new(Mutex::new(false));

    // Build the wezterm-term Terminal. Its writer is the ConditionalWriter,
    // which routes through the shared `writer` Arc but suppresses output when
    // a browser is attached (the browser handles DA/DSR replies itself).
    let term_writer = ConditionalWriter {
        inner: writer.clone(),
        has_subscriber: has_subscriber.clone(),
    };
    let term = Terminal::new(
        TerminalSize {
            rows: size.rows as usize,
            cols: size.cols as usize,
            pixel_width: 0,
            pixel_height: 0,
            dpi: 0,
        },
        Arc::new(MinimalConfig),
        "http-nu-pty",
        env!("CARGO_PKG_VERSION"),
        Box::new(term_writer),
    );
    let term = Arc::new(Mutex::new(term));

    let session = PtySession {
        master: pair.master,
        writer,
        child,
        meta: HashMap::new(),
        output_tx,
        term,
        has_subscriber,
    };
    Ok((session, reader))
}

/// Drain the pty master fd on a dedicated blocking thread. Each chunk is fed
/// to the wezterm-term Terminal (which updates the canonical virtual screen
/// state used by future attachers AND emits DA/DSR auto-replies through its
/// ConditionalWriter when no client is attached). The same chunk is also
/// forwarded raw to the attached SSE subscriber (if any). When `read` returns
/// 0 the child has exited; the thread removes the session from the map (if
/// `pty close` didn't already) and publishes `died` on the bus.
fn spawn_reader(
    sid: String,
    mut reader: Box<dyn Read + Send>,
    output_tx: Arc<Mutex<Option<std::sync::mpsc::Sender<Bytes>>>>,
    term: Arc<Mutex<Terminal>>,
    has_subscriber: Arc<Mutex<bool>>,
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
                    // Feed the chunk to wezterm-term BEFORE forwarding to the
                    // SSE subscriber, so an attach that races us either sees
                    // the snapshot before this chunk lands (and gets the chunk
                    // on its new channel) or sees the snapshot after (and
                    // skips this chunk on its new channel because the snapshot
                    // already reflects it). Hold the term lock across both.
                    let mut term_guard = term.lock().unwrap();
                    term_guard.advance_bytes(&buf[..n]);
                    // Forward raw bytes to the attached subscriber (if any).
                    // When a subscriber is attached the browser is itself a
                    // full VT emulator and natively replies to DA/DSR queries;
                    // ConditionalWriter suppresses wezterm-term's auto-reply
                    // while has_subscriber is true. When no subscriber is
                    // attached, wezterm-term's ConditionalWriter routes the
                    // replies straight to the slave so the shell unblocks.
                    let mut sub_lock = has_subscriber.lock().unwrap();
                    let mut tx_lock = output_tx.lock().unwrap();
                    if let Some(tx) = tx_lock.as_ref() {
                        if tx.send(chunk).is_err() {
                            // Receiver dropped (consumer disconnected). Clear
                            // the slot so future attaches don't race with a
                            // dead Sender, and let ConditionalWriter resume
                            // auto-replying to the slave.
                            *tx_lock = None;
                            *sub_lock = false;
                        }
                    }
                    drop(tx_lock);
                    drop(sub_lock);
                    drop(term_guard);
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
    //
    // On Linux, exec `/proc/self/exe` directly rather than the string from
    // current_exe(): if the on-disk binary has been replaced (e.g. a cargo
    // rebuild during a live server), readlink('/proc/self/exe') reports
    // the path with " (deleted)" appended and execve fails ENOENT. The
    // symlink itself still resolves to the running inode at exec time,
    // so the child gets the same binary the parent is running.
    #[cfg(target_os = "linux")]
    let self_exe = "/proc/self/exe".to_string();
    #[cfg(not(target_os = "linux"))]
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
            session.term.lock().unwrap().resize(TerminalSize {
                rows: rows as usize,
                cols: cols as usize,
                pixel_width: 0,
                pixel_height: 0,
                dpi: 0,
            });
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

        // Snapshot the wezterm-term screen and install a fresh Sender
        // atomically. Holding the term lock across both keeps the reader
        // thread from advancing bytes during this window: any chunk that
        // arrives after we drop the lock either lands in our new channel
        // (good) or was already absorbed into the snapshot (also good).
        // Replacing the Sender drops whatever the previous attach installed,
        // which closes that consumer's stream -- last attach wins. The
        // snapshot is a canonical ANSI sequence that reproduces the current
        // visible screen + cursor; partial fidelity for colors today (see
        // snapshot_terminal).
        let (rx, snapshot) = {
            let map = sessions().lock().unwrap();
            let session = map
                .get(&sid)
                .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;
            let term_guard = session.term.lock().unwrap();
            let snap = snapshot_terminal(&term_guard);
            let (tx, rx) = std::sync::mpsc::channel::<Bytes>();
            *session.output_tx.lock().unwrap() = Some(tx);
            *session.has_subscriber.lock().unwrap() = true;
            drop(term_guard);
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
