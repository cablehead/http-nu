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
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use base64::Engine as _;
use nu_engine::command_prelude::*;
use nu_protocol::{
    engine::Stack as NuStack, shell_error::generic::GenericError, ByteStream, ByteStreamType,
    Category, PipelineData, ShellError, Signature, Span, SyntaxShape, Type, Value,
};

use portable_pty::{native_pty_system, Child as PortableChild, CommandBuilder, MasterPty, PtySize};

use nix::pty::{openpty, Winsize};
use nix::sys::signal::{self, Signal};
use nix::sys::wait::waitpid;
use nix::unistd::{getpid, setsid, tcsetpgrp, ForkResult, Pid};

// --- session bookkeeping ----------------------------------------------------

enum MasterKind {
    Portable(Box<dyn MasterPty + Send>),
    Raw(OwnedFd),
}

impl MasterKind {
    fn resize(&self, size: PtySize, span: Span) -> Result<(), ShellError> {
        match self {
            Self::Portable(m) => m
                .resize(size)
                .map_err(|e| err(span, "pty resize failed", e.to_string())),
            Self::Raw(fd) => {
                let ws = libc::winsize {
                    ws_row: size.rows,
                    ws_col: size.cols,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                };
                let rc = unsafe { libc::ioctl(fd.as_raw_fd(), libc::TIOCSWINSZ, &ws) };
                if rc < 0 {
                    return Err(err(
                        span,
                        "pty resize failed",
                        std::io::Error::last_os_error().to_string(),
                    ));
                }
                Ok(())
            }
        }
    }

    fn clone_reader(&self, span: Span) -> Result<Box<dyn Read + Send>, ShellError> {
        match self {
            Self::Portable(m) => m
                .try_clone_reader()
                .map_err(|e| err(span, "clone_reader failed", e.to_string())),
            Self::Raw(fd) => {
                let cloned = fd
                    .try_clone()
                    .map_err(|e| err(span, "dup master fd failed", e.to_string()))?;
                Ok(Box::new(File::from(cloned)))
            }
        }
    }
}

enum ChildKind {
    Exec(Box<dyn PortableChild + Send + Sync>),
    Embedded(Pid),
}

impl ChildKind {
    fn kill(&mut self) {
        match self {
            Self::Exec(c) => {
                let _ = c.kill();
            }
            Self::Embedded(pid) => {
                let _ = signal::kill(*pid, Signal::SIGTERM);
            }
        }
    }

    fn wait(&mut self) {
        match self {
            Self::Exec(c) => {
                let _ = c.wait();
            }
            Self::Embedded(pid) => {
                let _ = waitpid(*pid, None);
            }
        }
    }
}

struct PtySession {
    master: MasterKind,
    writer: Box<dyn Write + Send>,
    reader_taken: bool,
    child: ChildKind,
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
        master: MasterKind::Portable(pair.master),
        writer,
        reader_taken: false,
        child: ChildKind::Exec(child),
    })
}

#[allow(clippy::result_large_err)]
fn open_embedded(
    engine_state: &EngineState,
    size: PtySize,
    span: Span,
) -> Result<PtySession, ShellError> {
    let ws = Winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let pty = openpty(&ws, None).map_err(|e| err(span, "openpty failed", e.to_string()))?;
    let master_fd: OwnedFd = pty.master;
    let slave_fd: OwnedFd = pty.slave;

    // Clone the engine state in the parent so the child inherits a snapshot
    // via fork's COW pages.
    let child_engine = engine_state.clone();

    // SAFETY: post-fork-pre-exec contract is relaxed here because we never
    // exec. Risks: (a) another tokio thread may hold a Mutex inside an Arc
    // inside EngineState at the fork instant; the child would deadlock if it
    // tried to acquire that lock. (b) Allocator state may be locked. Both
    // are rare in practice; keep the child's post-fork work minimal and let
    // evaluate_repl run.
    let fork_result =
        unsafe { nix::unistd::fork() }.map_err(|e| err(span, "fork failed", e.to_string()))?;

    match fork_result {
        ForkResult::Parent { child } => {
            drop(slave_fd);
            let writer_fd = master_fd
                .try_clone()
                .map_err(|e| err(span, "dup master fd failed", e.to_string()))?;
            let writer: Box<dyn Write + Send> = Box::new(File::from(writer_fd));
            Ok(PtySession {
                master: MasterKind::Raw(master_fd),
                writer,
                reader_taken: false,
                child: ChildKind::Embedded(child),
            })
        }
        ForkResult::Child => {
            // From here we must NEVER return into the caller's normal control
            // flow; exit on every path.
            drop(master_fd);
            run_embedded_nu(slave_fd, child_engine);
            unsafe { libc::_exit(0) };
        }
    }
}

/// Runs in the forked child. Sets the pty slave as the controlling terminal,
/// remaps stdio to it, closes inherited fds, then drives nu's REPL.
fn run_embedded_nu(slave_fd: OwnedFd, mut engine_state: EngineState) {
    let slave_raw = slave_fd.as_raw_fd();

    // New session, then make the slave our controlling tty.
    if setsid().is_err() {
        unsafe { libc::_exit(70) };
    }
    let rc = unsafe { libc::ioctl(slave_raw, libc::TIOCSCTTY as _, 0) };
    if rc < 0 {
        unsafe { libc::_exit(71) };
    }

    // Take slave -> 0, 1, 2 via libc::dup2 (nix's dup2 wants &mut OwnedFd,
    // which we can't synthesize for the std stdio fds).
    for target in [0_i32, 1, 2] {
        if unsafe { libc::dup2(slave_raw, target) } < 0 {
            unsafe { libc::_exit(73) };
        }
    }

    // Become the foreground process group of the controlling tty so SIGWINCH
    // (and other terminal signals) from TIOCSWINSZ on the master actually
    // reach us. Without this the kernel has no foreground group to signal.
    let pid = getpid();
    let stdin_fd = unsafe { std::os::fd::BorrowedFd::borrow_raw(0) };
    let _ = tcsetpgrp(stdin_fd, pid);

    drop(slave_fd);

    // Close every other fd we inherited from http-nu (listening sockets,
    // tokio epoll fd, sibling pty masters, log files, ...). Linux 5.9+.
    unsafe {
        let _ = libc::close_range(3, !0u32, 0);
    }

    // Bootstrap nu's default env.nu and config.nu (populates $env.config etc).
    // This is what nu's main calls `setup_config` for; we replicate the
    // no-user-files path here.
    let mut stack = NuStack::new();
    for kind in [
        nu_utils::ConfigFileKind::Env,
        nu_utils::ConfigFileKind::Config,
    ] {
        nu_cli::eval_source(
            &mut engine_state,
            &mut stack,
            kind.default().as_bytes(),
            kind.name(),
            nu_protocol::PipelineData::empty(),
            false,
        );
        let _ = engine_state.merge_env(&mut stack);
    }

    // Disable shell-integration OSC sequences. nushell emits OSC 133;P;k=r
    // and OSC 633 on every prompt; ghostty-web's wasm parser warns on them
    // and may eat subsequent bytes (rendering looks truncated).
    let disable_si = b"\
        $env.config.shell_integration.osc2 = false\n\
        $env.config.shell_integration.osc7 = false\n\
        $env.config.shell_integration.osc8 = false\n\
        $env.config.shell_integration.osc9_9 = false\n\
        $env.config.shell_integration.osc133 = false\n\
        $env.config.shell_integration.osc633 = false\n\
        $env.config.shell_integration.reset_application_mode = false\n\
    ";
    nu_cli::eval_source(
        &mut engine_state,
        &mut stack,
        disable_si,
        "disable_shell_integration",
        nu_protocol::PipelineData::empty(),
        false,
    );
    let _ = engine_state.merge_env(&mut stack);

    let _ = nu_cli::evaluate_repl(&mut engine_state, stack, None, None, Instant::now().into());
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
        session.master.resize(
            PtySize {
                cols: cols as u16,
                rows: rows as u16,
                pixel_width: 0,
                pixel_height: 0,
            },
            head,
        )?;

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
            session.master.clone_reader(head)?
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
            s.child.kill();
            s.child.wait();
        }
        Ok(PipelineData::Empty)
    }
}
