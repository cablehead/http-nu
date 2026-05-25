//! `pty` commands for http-nu: open/write/resize/view/close.
//!
//! Two backends:
//! - exec: fork+exec an external command via portable-pty
//! - embedded: fork the http-nu process, run nu's REPL in the child against
//!   a clone of the current EngineState. No external `nu` binary needed;
//!   the in-browser REPL has access to http-nu's custom commands.
//!
//! Sessions live in a process-wide map keyed by sid. The canonical screen
//! state lives in a server-side `wezterm_term::Terminal` per sid. Clients
//! subscribe via `pty view` and receive HTML grid snapshots over SSE,
//! morphed in place by Datastar.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

/// Lines of scrollback retained per pty session, server-side. Kept in sync
/// with the browser-side `scrollback:` option on `new Terminal({...})` via
/// the `$HTTP_NU.pty_scrollback_lines` const (see `engine::set_http_nu_const`),
/// which `sessions.html` templates into its constructor call. Changing this
/// number changes both sides at once.
pub const SCROLLBACK_LINES: usize = 3000;

/// Minimal TerminalConfiguration impl. Overrides only what we care about:
/// the color palette (required to render SGR), and `scrollback_size` so the
/// server-side history matches what the browser is willing to display.
#[derive(Debug, Default)]
struct MinimalConfig;

impl TerminalConfiguration for MinimalConfig {
    fn color_palette(&self) -> ColorPalette {
        ColorPalette::default()
    }

    fn scrollback_size(&self) -> usize {
        SCROLLBACK_LINES
    }
}

/// Writer wrapper that delegates through the same Arc<Mutex<...>> the rest of
/// the session uses. wezterm-term's auto-replies (DA1/DA2/DSR) always go
/// straight to the pty slave, since the browser is no longer a VT emulator
/// in projection mode -- it only renders the screen state we send it.
struct SharedWriter {
    inner: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.lock().unwrap().flush()
    }
}

/// Cheap structural equality on the attribute bits we render. CellAttributes
/// implements PartialEq, which compares all bits (including hyperlinks and
/// image refs we don't care about), so use a narrower check.
fn attrs_equiv(a: &CellAttributes, b: &CellAttributes) -> bool {
    a.attribute_bits_equal(b)
        && a.foreground() == b.foreground()
        && a.background() == b.background()
}

/// Map any palette index to xterm's canonical RGB.
/// 0..=15 is the standard 16-color palette (matches term.css `.f0..f15`),
/// 16..=231 is the 6x6x6 color cube, 232..=255 is the 24-step grayscale.
/// Used directly when emitting reverse-video cells (where we can't use the
/// `.fN`/`.bN` classes because the swap forces inline styles), and for
/// 256-color cells in the normal path.
fn palette_to_rgb(i: u8) -> (u8, u8, u8) {
    const PALETTE_16: [(u8, u8, u8); 16] = [
        (0x00, 0x00, 0x00), (0xcd, 0x00, 0x00), (0x00, 0xcd, 0x00), (0xcd, 0xcd, 0x00),
        (0x1e, 0x90, 0xff), (0xcd, 0x00, 0xcd), (0x00, 0xcd, 0xcd), (0xe5, 0xe5, 0xe5),
        (0x4d, 0x4d, 0x4d), (0xff, 0x54, 0x54), (0x54, 0xff, 0x54), (0xff, 0xff, 0x54),
        (0x54, 0x54, 0xff), (0xff, 0x54, 0xff), (0x54, 0xff, 0xff), (0xff, 0xff, 0xff),
    ];
    if i < 16 {
        return PALETTE_16[i as usize];
    }
    if i < 232 {
        let n = i - 16;
        let r = (n / 36) % 6;
        let g = (n / 6) % 6;
        let b = n % 6;
        let to_v = |c: u8| if c == 0 { 0 } else { 55 + c * 40 };
        return (to_v(r), to_v(g), to_v(b));
    }
    let l = 8u16 + (i as u16 - 232) * 10;
    let l = l.min(255) as u8;
    (l, l, l)
}

/// Append a CSS color declaration for `prop` (color / background) into
/// `out`, handling all four ColorAttribute variants. `default_var` is the
/// CSS variable name (e.g. "--term-fg") to use when the attribute is
/// Default; pass an empty string to skip emission entirely for the default
/// case (which is what the non-reverse path wants -- it relies on CSS
/// inheritance from the body).
fn append_color_inline(out: &mut String, prop: &str, c: ColorAttribute, default_var: &str) {
    match c {
        ColorAttribute::Default => {
            if !default_var.is_empty() {
                let _ = write!(out, "{prop}:var({default_var});");
            }
        }
        ColorAttribute::PaletteIndex(i) => {
            let (r, g, b) = palette_to_rgb(i);
            let _ = write!(out, "{prop}:#{r:02x}{g:02x}{b:02x};");
        }
        ColorAttribute::TrueColorWithDefaultFallback(rgb)
        | ColorAttribute::TrueColorWithPaletteFallback(rgb, _) => {
            let r = (rgb.0 * 255.0).round() as u8;
            let g = (rgb.1 * 255.0).round() as u8;
            let b = (rgb.2 * 255.0).round() as u8;
            let _ = write!(out, "{prop}:#{r:02x}{g:02x}{b:02x};");
        }
    }
}

/// Append CSS class fragments + inline style for a cell attribute set.
/// Bold/italic/underline/reverse/strikethrough become single-char classes.
/// Palette indices 0..16 become `f0`..`f15` / `b0`..`b15` so users can
/// theme the canonical 16 via CSS. Anything else (palette 16..=255,
/// truecolor) goes inline as `style="color:#rrggbb;..."`. Most TUIs lean
/// on the 16 palette, so the class-based path covers the common case and
/// brotli eats the repetition of the inline-style fallbacks.
fn cell_class_and_style(attrs: &CellAttributes) -> (String, String) {
    let mut classes = String::new();
    let mut style = String::new();

    match attrs.intensity() {
        Intensity::Bold => classes.push_str(" sb"),
        Intensity::Half => classes.push_str(" sd"),
        Intensity::Normal => {}
    }
    if attrs.italic() {
        classes.push_str(" si");
    }
    match attrs.underline() {
        Underline::None => {}
        _ => classes.push_str(" su"),
    }
    if attrs.invisible() {
        classes.push_str(" sx");
    }
    if attrs.strikethrough() {
        classes.push_str(" ss");
    }

    if attrs.reverse() {
        // Reverse video: swap foreground and background. Classes can't be
        // used here because the swap forces both color and background to be
        // explicit -- a `.f1` class would otherwise set color to red even
        // though we want the original background as the new foreground.
        // Default fg/bg map to CSS variables so the theme stays in charge.
        append_color_inline(&mut style, "color", attrs.background(), "--term-bg");
        append_color_inline(&mut style, "background", attrs.foreground(), "--term-fg");
    } else {
        // Normal path: classes for the 16-palette (themable via CSS),
        // inline RGB for 256-color and truecolor.
        match attrs.foreground() {
            ColorAttribute::Default => {}
            ColorAttribute::PaletteIndex(i) if i < 16 => {
                let _ = write!(classes, " f{i}");
            }
            other => append_color_inline(&mut style, "color", other, ""),
        }
        match attrs.background() {
            ColorAttribute::Default => {}
            ColorAttribute::PaletteIndex(i) if i < 16 => {
                let _ = write!(classes, " b{i}");
            }
            other => append_color_inline(&mut style, "background", other, ""),
        }
    }

    (classes, style)
}

/// Escape a string for use inside HTML text. Just the four characters that
/// matter inside `<span>...</span>` plus quote in case it ever leaks.
fn html_escape(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

/// One rendered terminal frame: the grid HTML plus the metadata the client
/// surfaces as signals (dimensions and OSC title) rather than reading off
/// the DOM.
struct GridFrame {
    html: String,
    cols: usize,
    rows: usize,
    title: String,
}

/// Render the terminal's retained scrollback + visible screen + cursor as
/// an HTML grid suitable for morphing into `#grid`. Each row is
/// `<div class="row" id="r-{N}">...</div>` where N is the row's phys index;
/// idiomorph matches rows by id and only morphs the ones that changed.
/// Cells inside a row are run-length encoded into `<span class="...">`
/// runs sharing the same attribute set.
///
/// All retained scrollback lines are emitted, not just the visible region.
/// CSS scrolls the grid container; the client auto-sticks to the bottom
/// unless the user has scrolled up. When scrollback fills past the cap, the
/// oldest line drops off and every row's phys index shifts by one, which
/// makes idiomorph morph each row's content -- brotli on the wire keeps
/// the bandwidth cost tolerable.
fn render_grid_html(term: &Terminal, target: &str) -> GridFrame {
    let size = term.get_size();
    let cols = size.cols;
    let phys_rows = size.rows;
    let cursor = term.cursor_pos();
    let screen = term.screen();
    let total = screen.scrollback_rows();
    let visible_start = total.saturating_sub(phys_rows);
    let lines = screen.lines_in_phys_range(0..total);

    // Cursor.y is relative to the visible region; translate to an absolute
    // phys row index so the cursor sits on the right row when the grid
    // includes scrollback above it.
    let cursor_row = visible_start + cursor.y as usize;
    let cursor_col = cursor.x;

    let default_attrs = CellAttributes::default();

    let title = term.get_title().to_string();

    let mut out = String::new();
    let _ = write!(
        out,
        "<div id=\"{target}\" data-cols=\"{cols}\" data-rows=\"{phys_rows}\" data-total=\"{total}\">"
    );

    for (row_idx, line) in lines.iter().enumerate() {
        let _ = write!(out, "<div class=\"row\" id=\"r-{row_idx}\">");

        // Materialize the row into (text, attrs, is_cursor) per column so we
        // can run-length encode in one pass without worrying about wide-cell
        // gaps. Default-fill any column the line didn't write to.
        let mut cells: Vec<(String, CellAttributes, bool)> = (0..cols)
            .map(|_| (" ".to_string(), default_attrs.clone(), false))
            .collect();
        for cell_ref in line.visible_cells() {
            let col = cell_ref.cell_index();
            if col >= cols {
                break;
            }
            let s = cell_ref.str();
            let glyph = if s.is_empty() { " ".to_string() } else { s.to_string() };
            cells[col] = (glyph, cell_ref.attrs().clone(), false);
        }
        if row_idx == cursor_row && cursor_col < cols {
            cells[cursor_col].2 = true;
        }

        // Run-length encode: walk and group consecutive cells with equal
        // (attrs, is_cursor). The cursor cell always breaks the run since it
        // gets a distinct class.
        let mut i = 0;
        while i < cells.len() {
            let (_, ref a, ca) = cells[i];
            let mut j = i + 1;
            while j < cells.len() {
                let (_, ref b, cb) = cells[j];
                if attrs_equiv(a, b) && ca == cb {
                    j += 1;
                } else {
                    break;
                }
            }

            let (mut classes, style) = cell_class_and_style(a);
            if ca {
                classes.push_str(" cursor");
            }
            let text_len: usize = cells[i..j].iter().map(|c| c.0.len()).sum();
            let mut text = String::with_capacity(text_len);
            for c in &cells[i..j] {
                text.push_str(&c.0);
            }
            let mut escaped = String::with_capacity(text.len());
            html_escape(&text, &mut escaped);

            if classes.is_empty() && style.is_empty() {
                // No attrs, no cursor -- emit text bare. Saves bytes on
                // ordinary whitespace runs, which dominate empty terminals.
                out.push_str(&escaped);
            } else {
                out.push_str("<span class=\"c");
                out.push_str(&classes);
                out.push('"');
                if !style.is_empty() {
                    out.push_str(" style=\"");
                    out.push_str(&style);
                    out.push('"');
                }
                out.push('>');
                out.push_str(&escaped);
                out.push_str("</span>");
            }
            i = j;
        }
        out.push_str("</div>");
    }
    out.push_str("</div>");
    GridFrame {
        html: out,
        cols,
        rows: phys_rows,
        title,
    }
}

/// Append a JSON string literal (with surrounding quotes) for `s` into `out`.
fn json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

// --- session bookkeeping ----------------------------------------------------

const PTY_EVENTS_TOPIC: &str = "pty.events";

struct PtySession {
    master: Box<dyn MasterPty + Send>,
    // Shared with the wezterm-term Terminal (for DA1/DA2/DSR auto-replies)
    // and with `pty write` (for user input). Both contend on the same Mutex,
    // which is fine -- this is a low-throughput interactive path.
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Box<dyn PortableChild + Send + Sync>,
    meta: HashMap<String, Value>,
    // Canonical virtual screen. Fed by the reader thread, snapshotted by
    // `pty view` subscribers.
    term: Arc<Mutex<Terminal>>,
    // Generation counter + condvar. The reader thread bumps the counter
    // after each `advance_bytes` and notifies all waiters. Every `pty view`
    // subscriber holds its own `last_seen_gen` and wakes when this advances,
    // then renders the current screen. Many subscribers per sid are fine --
    // notify_all wakes them all.
    dirty: Arc<(Mutex<u64>, Condvar)>,
    // ms since epoch of the most recent write to this session's stdin. Seeded
    // to creation time so a freshly-spawned session sorts above quiet ones.
    // Bumped by `bump_last_input`, read by `pty list` so the sidebar can
    // order tabs by recent activity.
    last_input_ms: AtomicU64,
    // Carried so `bump_last_input` can publish a `touched` event when this
    // session's bump moves it above any other session in the sort order.
    bus: Arc<Bus>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Bump this session's `last_input_ms` to now. If the bump moves it above
/// every other session's timestamp (i.e. the sort order's top sid changed),
/// publish `pty.events {event: "touched", sid}` so subscribers re-render.
/// Quiet bumps (this sid was already top) are skipped to keep the bus quiet
/// under sustained typing in the foreground tab.
fn bump_last_input(sid: &str) {
    let now = now_ms();
    let bus_to_notify = {
        let map = sessions().lock().unwrap();
        let Some(session) = map.get(sid) else {
            return;
        };
        let prev = session.last_input_ms.swap(now, Ordering::Relaxed);
        let was_top = map.iter().filter(|(k, _)| k.as_str() != sid).all(|(_, s)| {
            s.last_input_ms.load(Ordering::Relaxed) <= prev
        });
        if was_top {
            None
        } else {
            Some(session.bus.clone())
        }
    };
    if let Some(bus) = bus_to_notify {
        let span = Span::unknown();
        bus.publish(
            PTY_EVENTS_TOPIC,
            Value::record(
                record! {
                    "event" => Value::string("touched", span),
                    "sid" => Value::string(sid, span),
                },
                span,
            ),
        );
    }
}

fn sessions() -> &'static Mutex<HashMap<String, PtySession>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, PtySession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

#[derive(Debug)]
pub(crate) enum WriteInputError {
    NotFound,
    Io(std::io::Error),
}

/// Single entry point for writing user input into a pty session. Used by the
/// handler-layer fast-path for POST /pty/input (which skips the nushell eval
/// thread) and by `pty write` (the nushell-side equivalent). Both callers
/// route through here so the bump + ordering ping happens in exactly one
/// place. The writer's Mutex is shared with the wezterm-term auto-reply
/// path; the lock is held only across this single write_all + flush.
pub(crate) fn write_input(sid: &str, bytes: &[u8]) -> Result<(), WriteInputError> {
    let writer = {
        let map = sessions().lock().unwrap();
        let session = map.get(sid).ok_or(WriteInputError::NotFound)?;
        session.writer.clone()
    };
    let mut w = writer.lock().unwrap();
    w.write_all(bytes).map_err(WriteInputError::Io)?;
    let _ = w.flush();
    drop(w);
    bump_last_input(sid);
    Ok(())
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
            open_embedded(engine_state, size, head, self.bus.clone())?
        } else {
            let cmd = cmd.ok_or_else(|| err(head, "missing cmd", "required without --embedded"))?;
            open_exec(&cmd, args, size, head, self.bus.clone())?
        };

        let sid = scru128::new().to_string();
        let (cols_n, rows_n) = (size.cols as i64, size.rows as i64);
        let term = session.term.clone();
        let dirty = session.dirty.clone();
        sessions().lock().unwrap().insert(sid.clone(), session);

        // Spawn the reader thread now (after insert) so it can self-reap
        // by sid when the child eventually exits.
        spawn_reader(sid.clone(), reader, term, dirty, self.bus.clone());

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
    bus: Arc<Bus>,
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

    let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(raw_writer));

    // Build the wezterm-term Terminal. Its writer is a SharedWriter pointing
    // at the same master pty fd that user input writes to. Auto-replies for
    // DA1/DA2/DSR queries always go straight to the slave -- the browser is
    // no longer a VT emulator in projection mode.
    let term_writer = SharedWriter { inner: writer.clone() };
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
    let dirty = Arc::new((Mutex::new(0u64), Condvar::new()));

    let session = PtySession {
        master: pair.master,
        writer,
        child,
        meta: HashMap::new(),
        term,
        dirty,
        last_input_ms: AtomicU64::new(now_ms()),
        bus,
    };
    Ok((session, reader))
}

/// Drain the pty master fd on a dedicated blocking thread. Each chunk is fed
/// to the wezterm-term Terminal (which updates the canonical virtual screen
/// state AND emits DA/DSR auto-replies straight to the slave through the
/// shared writer). After advancing, the dirty counter is bumped and all
/// `pty view` subscribers are notified so they can re-render. When `read`
/// returns 0 the child has exited; the thread removes the session from the
/// map (if `pty close` didn't already) and publishes `died` on the bus.
fn spawn_reader(
    sid: String,
    mut reader: Box<dyn Read + Send>,
    term: Arc<Mutex<Terminal>>,
    dirty: Arc<(Mutex<u64>, Condvar)>,
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
                    if let Some(f) = dump_file.as_mut() {
                        let _ = writeln!(f, "[{sid}] {}", escape_bytes(&buf[..n]));
                        let _ = f.flush();
                    }
                    {
                        let mut term_guard = term.lock().unwrap();
                        term_guard.advance_bytes(&buf[..n]);
                    }
                    let (lock, cv) = &*dirty;
                    {
                        let mut g = lock.lock().unwrap();
                        *g = g.wrapping_add(1);
                    }
                    cv.notify_all();
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
            // Wake any view subscribers so they exit promptly rather than
            // sitting on the condvar until next heartbeat.
            let (lock, cv) = &*s.dirty;
            {
                let mut g = lock.lock().unwrap();
                *g = g.wrapping_add(1);
            }
            cv.notify_all();

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
    bus: Arc<Bus>,
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
    open_exec(&self_exe, Some(vec!["repl".to_string()]), size, span, bus)
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

        write_input(&sid, &bytes).map_err(|e| match e {
            WriteInputError::NotFound => err(head, format!("no pty session: {sid}"), ""),
            WriteInputError::Io(e) => err(head, "pty write failed", e.to_string()),
        })?;

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
            // Wake subscribers so the next frame reflects the new grid
            // dimensions even if the program isn't generating output.
            let (lock, cv) = &*session.dirty;
            {
                let mut g = lock.lock().unwrap();
                *g = g.wrapping_add(1);
            }
            cv.notify_all();
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

// --- pty view ---------------------------------------------------------------

/// Coalescing window: after a dirty notify wakes us, sleep this long before
/// rendering so a burst of pty output (e.g. `cat large.txt`) collapses into
/// one frame rather than one per chunk.
const VIEW_COALESCE: Duration = Duration::from_millis(16);

/// How long to wait on the condvar before emitting an SSE comment heartbeat.
/// Keeps intermediaries (proxies, browser) from closing an idle connection,
/// and bounds the time a stale subscriber holds the term lock.
const VIEW_HEARTBEAT: Duration = Duration::from_secs(15);

#[derive(Clone)]
pub struct PtyViewCommand;

impl PtyViewCommand {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PtyViewCommand {
    fn default() -> Self {
        Self::new()
    }
}

impl Command for PtyViewCommand {
    fn name(&self) -> &str {
        "pty view"
    }

    fn description(&self) -> &str {
        "Stream the pty's visible screen as morph-able HTML grid frames over SSE (Datastar datastar-patch-elements events)"
    }

    fn signature(&self) -> Signature {
        Signature::build("pty view")
            .required("sid", SyntaxShape::String, "session id")
            .named(
                "target",
                SyntaxShape::String,
                "id of the morph-target element (default 'grid'); use a unique id per pane when several views render at once",
                None,
            )
            .switch(
                "no-signals",
                "suppress the termCols/termRows/termTitle signal patch (use when multiple views would otherwise collide on the same global signals)",
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
        let sid: String = call.req(engine_state, stack, 0)?;
        let target: String = call
            .get_flag(engine_state, stack, "target")?
            .unwrap_or_else(|| "grid".to_string());
        let no_signals = call.has_flag(engine_state, stack, "no-signals")?;

        // Resolve term + dirty handles once. The session may go away while we
        // stream; we treat that as natural EOF by checking `sessions()` each
        // iteration and bailing if the sid is gone.
        let (term, dirty) = {
            let map = sessions().lock().unwrap();
            let session = map
                .get(&sid)
                .ok_or_else(|| err(head, format!("no pty session: {sid}"), ""))?;
            (session.term.clone(), session.dirty.clone())
        };

        let mut last_gen: u64 = 0;
        let mut sent_initial = false;
        // Last (cols, rows, title) emitted as signals; only re-emit the
        // patch-signals event when one changes so keystroke frames don't
        // carry a redundant signal patch.
        let mut last_meta: Option<(usize, usize, String)> = None;
        let sid_owned = sid.clone();

        let stream = ByteStream::from_fn(
            head,
            engine_state.signals().clone(),
            ByteStreamType::String,
            move |buffer: &mut Vec<u8>| {
                // Bail when the session is gone (closed or child exited).
                if !sessions().lock().unwrap().contains_key(&sid_owned) {
                    return Ok(false);
                }

                if sent_initial {
                    // Wait for the dirty counter to advance or for the
                    // heartbeat timeout to fire. notify_all from the reader
                    // thread wakes us with the new generation.
                    let (lock, cv) = &*dirty;
                    let mut guard = lock.lock().unwrap();
                    let mut emitted_heartbeat = false;
                    while *guard == last_gen {
                        let (g, timeout) = cv
                            .wait_timeout(guard, VIEW_HEARTBEAT)
                            .unwrap();
                        guard = g;
                        if timeout.timed_out() {
                            // Emit an SSE comment so proxies don't drop us.
                            buffer.extend_from_slice(b": hb\n\n");
                            emitted_heartbeat = true;
                            break;
                        }
                    }
                    drop(guard);
                    if emitted_heartbeat {
                        return Ok(true);
                    }
                    // Coalesce: sleep briefly so a burst of chunks collapses
                    // into one frame rather than one frame per chunk.
                    std::thread::sleep(VIEW_COALESCE);
                }

                // Snapshot the latest generation + render the screen.
                let (lock, _cv) = &*dirty;
                let gen_now = *lock.lock().unwrap();
                let frame = {
                    let term_guard = term.lock().unwrap();
                    render_grid_html(&term_guard, &target)
                };
                last_gen = gen_now;
                sent_initial = true;

                emit_patch_elements(buffer, &frame.html);

                // Surface dims + title as signals so the client binds them
                // declaratively (status line, document.title) instead of
                // observing DOM attributes. Only emit on change, and never
                // when several views share the page (--no-signals) since the
                // signals are global and would clobber each other.
                if !no_signals {
                    let meta = (frame.cols, frame.rows, frame.title);
                    if last_meta.as_ref() != Some(&meta) {
                        emit_patch_signals(buffer, meta.0, meta.1, &meta.2);
                        last_meta = Some(meta);
                    }
                }
                Ok(true)
            },
        );

        Ok(PipelineData::ByteStream(stream, None))
    }
}

/// Wrap a chunk of HTML as a `datastar-patch-elements` SSE event. The
/// `elements` data line carries the HTML to morph; Datastar matches by id
/// (we use `<div id="grid">` as the morph target).
fn emit_patch_elements(buffer: &mut Vec<u8>, html: &str) {
    buffer.extend_from_slice(b"event: datastar-patch-elements\n");
    // The HTML never contains a literal newline (we don't pretty-print it),
    // so a single `data: elements <...>` line is correct. Defensive: if a
    // newline ever leaks in, split it into multiple data lines.
    if html.contains('\n') {
        for line in html.split('\n') {
            buffer.extend_from_slice(b"data: elements ");
            buffer.extend_from_slice(line.as_bytes());
            buffer.extend_from_slice(b"\n");
        }
    } else {
        buffer.extend_from_slice(b"data: elements ");
        buffer.extend_from_slice(html.as_bytes());
        buffer.extend_from_slice(b"\n");
    }
    buffer.extend_from_slice(b"\n");
}

/// Emit a `datastar-patch-signals` event carrying the frame's dimensions
/// and OSC title, so the client surfaces them via signal bindings rather
/// than reading DOM attributes. Signals are prefixed `term*` so they don't
/// collide with surface-level signals (e.g. the sessions window `$title`).
fn emit_patch_signals(buffer: &mut Vec<u8>, cols: usize, rows: usize, title: &str) {
    let mut signals = String::new();
    let _ = write!(signals, "{{termCols:{cols},termRows:{rows},termTitle:");
    json_string(&mut signals, title);
    signals.push('}');
    buffer.extend_from_slice(b"event: datastar-patch-signals\n");
    buffer.extend_from_slice(b"data: signals ");
    buffer.extend_from_slice(signals.as_bytes());
    buffer.extend_from_slice(b"\n\n");
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
            let last_input = s.last_input_ms.load(Ordering::Relaxed) as i64;
            rows.push(Value::record(
                record! {
                    "sid" => Value::string(sid, head),
                    "cols" => Value::int(cols, head),
                    "rows" => Value::int(rs, head),
                    "meta" => Value::record(meta_rec, head),
                    "last_input_ms" => Value::int(last_input, head),
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
