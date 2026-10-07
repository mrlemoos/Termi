//! One tab = one PTY + alacritty grid. Rendering and key mapping are free functions.

use eframe::egui;
use std::borrow::Cow;
use std::collections::HashMap;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::tty;
use alacritty_terminal::vte::ansi::{Color, CursorShape, Rgb};
use egui::{Color32, FontId, Key, Modifiers, Painter, Pos2, Rect, Stroke, Vec2, vec2};

use crate::agent::{self, Agent, State};

#[derive(Clone)]
pub struct Listener {
    tx: Sender<Event>,
    ctx: egui::Context,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        let _ = self.tx.send(event);
        self.ctx.request_repaint();
    }
}

pub struct Tab {
    pub id: u64,
    pub term: Arc<FairMutex<Term<Listener>>>,
    loop_tx: EventLoopSender,
    events: Receiver<Event>,
    pid: u32,
    fd: i32,
    pub title: String,
    /// Foreground process group that set `title`; a title only names the session of whoever set it.
    title_pgid: i32,
    fg: i32,
    pub cmd: String,
    pub cwd: PathBuf,
    pub agent: Option<&'static dyn Agent>,
    pub state: State,
    /// Finished working while you weren't looking; cleared when the tab is viewed.
    pub done: bool,
    pub exited: bool,
    size: WindowSize,
}

pub struct Cell {
    pub w: f32,
    pub h: f32,
}

impl Tab {
    pub fn spawn(id: u64, cwd: PathBuf, ctx: &egui::Context) -> std::io::Result<Tab> {
        let size = WindowSize { num_lines: 24, num_cols: 80, cell_width: 8, cell_height: 16 };
        let env = HashMap::from([
            ("TERM_PROGRAM".into(), "Termi".into()),
            ("COLORTERM".into(), "truecolor".into()),
            ("TERMI_TAB".into(), id.to_string()),
            ("TERMI_STATE_DIR".into(), agent::state_dir().to_string_lossy().into_owned()),
        ]);
        let opts = tty::Options { shell: None, working_directory: Some(cwd.clone()), drain_on_exit: false, env };
        let pty = tty::new(&opts, size, id)?;
        let (pid, fd) = (pty.child().id(), pty.file().as_raw_fd());

        let (tx, events) = channel();
        let listener = Listener { tx, ctx: ctx.clone() };
        let dims = TermSize::new(size.num_cols as usize, size.num_lines as usize);
        let term = Arc::new(FairMutex::new(Term::new(Config::default(), &dims, listener.clone())));
        let ev_loop = EventLoop::new(term.clone(), listener, pty, false, false)?;
        let loop_tx = ev_loop.channel();
        ev_loop.spawn();

        Ok(Tab {
            id, term, loop_tx, events, pid, fd, size, cwd,
            title: String::new(), title_pgid: 0, fg: 0, cmd: String::new(), agent: None, state: State::Idle, done: false, exited: false,
        })
    }

    pub fn write(&self, bytes: impl Into<Vec<u8>>) {
        let _ = self.loop_tx.send(Msg::Input(Cow::Owned(bytes.into())));
    }

    /// Typed input: straight to the PTY, snap back to the live screen.
    pub fn paste_raw(&self, text: &str) {
        self.write(text.as_bytes());
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    /// Text as the program wants to receive a paste (bracketed if it asked for it).
    pub fn paste(&self, text: &str) {
        let bracketed = self.term.lock().mode().contains(TermMode::BRACKETED_PASTE);
        let text = text.replace("\r\n", "\r").replace('\n', "\r");
        self.write(if bracketed { format!("\x1b[200~{text}\x1b[201~") } else { text });
        self.term.lock().scroll_display(Scroll::Bottom);
    }

    pub fn resize(&mut self, cols: u16, lines: u16, cell: &Cell) {
        if (cols, lines) == (self.size.num_cols, self.size.num_lines) || cols == 0 || lines == 0 {
            return;
        }
        self.size = WindowSize { num_cols: cols, num_lines: lines, cell_width: cell.w as u16, cell_height: cell.h as u16 };
        self.term.lock().resize(TermSize::new(cols as usize, lines as usize));
        let _ = self.loop_tx.send(Msg::Resize(self.size));
    }

    /// Drain events coming from the PTY thread.
    pub fn pump(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                Event::Title(t) => {
                    self.title = t;
                    self.title_pgid = unsafe { libc::tcgetpgrp(self.fd) };
                }
                Event::ResetTitle => self.title.clear(),
                Event::PtyWrite(s) => self.write(s),
                Event::ClipboardStore(_, s) => ctx.copy_text(s),
                Event::ColorRequest(i, fmt) => {
                    let c = self.term.lock().colors()[i].unwrap_or_else(|| to_rgb(index_color(i)));
                    self.write(fmt(c));
                }
                Event::TextAreaSizeRequest(fmt) => self.write(fmt(self.size)),
                Event::ChildExit(_) | Event::Exit => self.exited = true,
                _ => {}
            }
        }
    }

    /// Foreground process, cwd, agent + its state. Called ~1/s.
    pub fn poll(&mut self) {
        self.fg = unsafe { libc::tcgetpgrp(self.fd) };
        let fg = if self.fg > 0 { self.fg as u32 } else { self.pid };
        // ponytail: one `ps` per tab per poll; sysctl(KERN_PROCARGS2) if this ever shows up in a profile.
        self.cmd = std::process::Command::new("ps")
            .args(["-o", "command=", "-p", &fg.to_string()])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
            .unwrap_or_default();
        if let Some(cwd) = proc_cwd(fg).or_else(|| proc_cwd(self.pid)) {
            self.cwd = cwd;
        }
        self.agent = agent::detect(&self.cmd);
        let prev = self.state;
        self.state = match self.agent {
            Some(a) => agent::resolve(a, &self.screen_text(), self.id),
            None => State::Idle,
        };
        if let (Some(a), Some(_)) = (self.agent, std::env::var_os("TERMI_DEBUG")) {
            debug_log(self.id, a.name(), self.state, &self.screen_text());
        }
        // ponytail: 1s poll, so a turn shorter than that can finish unseen; hooks catch those.
        self.done = match self.state {
            State::Idle => self.done || (prev == State::Working && self.agent.is_some()),
            _ => false,
        };
    }

    fn screen_text(&self) -> String {
        let term = self.term.lock();
        let mut out = String::new();
        let mut line = None;
        for c in term.grid().display_iter() {
            if line != Some(c.point.line) {
                out.push('\n');
                line = Some(c.point.line);
            }
            out.push(c.c);
        }
        out
    }

    /// Label for the status line.
    pub fn label(&self) -> String {
        match self.agent {
            Some(a) => match session_name(&self.title, a.name()).filter(|_| self.title_pgid == self.fg) {
                Some(s) => format!("{}: {s}", a.name()),
                None => a.name().to_owned(),
            },
            None => self.cmd.split_whitespace().next().unwrap_or("zsh").rsplit('/').next().unwrap_or("").trim_start_matches('-').to_owned(),
        }
    }

    pub fn start_selection(&self, p: Pos2, origin: Pos2, cell: &Cell) {
        let mut t = self.term.lock();
        let (point, side) = to_point(&t, p, origin, cell);
        t.selection = Some(Selection::new(SelectionType::Simple, point, side));
    }

    pub fn update_selection(&self, p: Pos2, origin: Pos2, cell: &Cell) {
        let mut t = self.term.lock();
        let (point, side) = to_point(&t, p, origin, cell);
        if let Some(s) = t.selection.as_mut() {
            s.update(point, side);
        }
    }

    pub fn scroll(&self, lines: i32) {
        let alt = self.term.lock().mode().contains(TermMode::ALT_SCREEN);
        if alt {
            let key = if lines > 0 { "\x1bOA" } else { "\x1bOB" };
            self.write(key.repeat(lines.unsigned_abs() as usize));
        } else {
            self.term.lock().scroll_display(Scroll::Delta(lines));
        }
    }
}

impl Drop for Tab {
    /// Stops the IO thread, which drops the Pty, which hangs up the shell.
    fn drop(&mut self) {
        let _ = self.loop_tx.send(Msg::Shutdown);
    }
}

/// TERMI_DEBUG=1: append each agent poll (state + bottom of screen) to $TMPDIR/termi/debug.log,
/// the raw material for tuning `busy_marks`/`ask_marks`.
fn debug_log(tab: u64, agent: &str, state: State, screen: &str) {
    use std::io::Write;
    let lines: Vec<&str> = screen.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(15)..].join("\n");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(agent::state_dir().join("debug.log")) {
        let _ = writeln!(f, "== tab {tab} {agent} {state:?}\n{tail}");
    }
}

/// Session name from an agent's terminal title: drop spinner/status glyphs,
/// ignore the agent's default title ("Claude Code"), cap the length.
// ponytail: "title mentions the agent = default title" heuristic.
fn session_name(title: &str, agent: &str) -> Option<String> {
    let name = title.trim_start_matches(|c: char| !c.is_alphanumeric()).trim();
    if name.is_empty() || name.to_lowercase().contains(agent) {
        return None;
    }
    Some(match name.char_indices().nth(24) {
        Some((i, _)) => format!("{}…", &name[..i]),
        None => name.to_owned(),
    })
}

fn to_point<T>(t: &Term<T>, p: Pos2, origin: Pos2, cell: &Cell) -> (Point, Side) {
    let x = ((p.x - origin.x) / cell.w).max(0.0);
    let col = (x as usize).min(t.columns() - 1);
    let row = (((p.y - origin.y) / cell.h).max(0.0) as i32).min(t.screen_lines() as i32 - 1);
    let side = if x.fract() < 0.5 { Side::Left } else { Side::Right };
    (Point::new(Line(row - t.grid().display_offset() as i32), Column(col)), side)
}

fn proc_cwd(pid: u32) -> Option<PathBuf> {
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as i32;
    let n = unsafe {
        libc::proc_pidinfo(pid as i32, libc::PROC_PIDVNODEPATHINFO, 0, &mut info as *mut _ as *mut _, size)
    };
    if n != size {
        return None;
    }
    let raw = unsafe { std::ffi::CStr::from_ptr(info.pvi_cdir.vip_path.as_ptr() as *const _) };
    Some(PathBuf::from(raw.to_str().ok()?))
}

// ---------- colors: plain xterm, black background ----------

pub const BG: Color32 = Color32::BLACK;
pub const FG: Color32 = Color32::from_rgb(0xe5, 0xe5, 0xe5);

const ANSI: [u32; 16] = [
    0x000000, 0xcd0000, 0x00cd00, 0xcdcd00, 0x0000ee, 0xcd00cd, 0x00cdcd, 0xe5e5e5,
    0x7f7f7f, 0xff0000, 0x00ff00, 0xffff00, 0x5c5cff, 0xff00ff, 0x00ffff, 0xffffff,
];

fn hex(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

fn to_rgb(c: Color32) -> Rgb {
    Rgb { r: c.r(), g: c.g(), b: c.b() }
}

fn dim(c: Color32) -> Color32 {
    let d = |v: u8| (v as u16 * 2 / 3) as u8;
    Color32::from_rgb(d(c.r()), d(c.g()), d(c.b()))
}

/// xterm 256-color table + alacritty's extra named slots.
fn index_color(i: usize) -> Color32 {
    const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match i {
        0..16 => hex(ANSI[i]),
        16..232 => {
            let i = i - 16;
            Color32::from_rgb(CUBE[i / 36], CUBE[i / 6 % 6], CUBE[i % 6])
        }
        232..256 => Color32::from_gray(8 + 10 * (i - 232) as u8),
        256 | 258 => FG,
        257 => BG,
        259..267 => dim(hex(ANSI[i - 259])),
        267 => Color32::WHITE,
        _ => dim(FG),
    }
}

fn resolve(c: Color, colors: &alacritty_terminal::term::color::Colors, bold: bool) -> Color32 {
    let i = match c {
        Color::Spec(rgb) => return Color32::from_rgb(rgb.r, rgb.g, rgb.b),
        Color::Indexed(i) => i as usize,
        Color::Named(n) => n as usize,
    };
    // bold-is-bright, like every default terminal
    let i = if bold && i < 8 { i + 8 } else { i };
    colors[i].map(|c| Color32::from_rgb(c.r, c.g, c.b)).unwrap_or_else(|| index_color(i))
}

// ---------- render ----------

pub struct Fonts {
    pub regular: FontId,
    pub bold: FontId,
}

pub fn paint(tab: &Tab, painter: &Painter, origin: Pos2, cell: &Cell, fonts: &Fonts, focused: bool) {
    let term = tab.term.lock();
    let content = term.renderable_content();
    let off = content.display_offset as i32;
    let cursor = content.cursor;
    let at = |row: i32, col: usize| origin + vec2(col as f32 * cell.w, row as f32 * cell.h);

    // ponytail: ASCII runs painted as one string, everything else per cell; glyph atlas renderer if perf bites.
    let mut run: Option<(i32, usize, Color32, bool, String)> = None;
    let flush = |run: &mut Option<(i32, usize, Color32, bool, String)>| {
        if let Some((row, col, fg, bold, text)) = run.take() {
            painter.text(at(row, col), egui::Align2::LEFT_TOP, text, if bold { fonts.bold.clone() } else { fonts.regular.clone() }, fg);
        }
    };

    for c in content.display_iter {
        let row = c.point.line.0 + off;
        let col = c.point.column.0;
        let flags = c.cell.flags;
        if flags.contains(Flags::WIDE_CHAR_SPACER) {
            continue;
        }
        let bold = flags.contains(Flags::BOLD);
        let mut fg = resolve(c.cell.fg, content.colors, bold);
        let mut bg = resolve(c.cell.bg, content.colors, false);
        if flags.contains(Flags::DIM) {
            fg = dim(fg);
        }
        let selected = content.selection.is_some_and(|s| s.contains(c.point));
        let is_cursor = focused && c.point == cursor.point && cursor.shape == CursorShape::Block;
        if flags.contains(Flags::INVERSE) ^ selected ^ is_cursor {
            std::mem::swap(&mut fg, &mut bg);
        }
        let width = if flags.contains(Flags::WIDE_CHAR) { 2.0 } else { 1.0 };
        if bg != BG {
            painter.rect_filled(Rect::from_min_size(at(row, col), vec2(cell.w * width, cell.h)), 0.0, bg);
        }
        if flags.contains(Flags::UNDERLINE) {
            let y = at(row, col).y + cell.h - 1.0;
            painter.hline(at(row, col).x..=at(row, col).x + cell.w, y, Stroke::new(1.0, fg));
        }
        let ch = if flags.contains(Flags::HIDDEN) { ' ' } else { c.cell.c };
        let continues = run.as_ref().is_some_and(|(r, start, f, b, t)| {
            *r == row && start + t.len() == col && *f == fg && *b == bold
        });
        if !ch.is_ascii() || !continues {
            flush(&mut run);
        }
        if ch.is_ascii() {
            run.get_or_insert_with(|| (row, col, fg, bold, String::new())).4.push(ch);
        } else {
            painter.text(at(row, col), egui::Align2::LEFT_TOP, ch, if bold { fonts.bold.clone() } else { fonts.regular.clone() }, fg);
        }
    }
    flush(&mut run);

    let p = at(cursor.point.line.0 + off, cursor.point.column.0);
    let r = Rect::from_min_size(p, vec2(cell.w, cell.h));
    match cursor.shape {
        CursorShape::Block if !focused => { painter.rect_stroke(r, 0.0, Stroke::new(1.0, FG), egui::StrokeKind::Inside); }
        CursorShape::HollowBlock => { painter.rect_stroke(r, 0.0, Stroke::new(1.0, FG), egui::StrokeKind::Inside); }
        CursorShape::Beam => { painter.rect_filled(Rect::from_min_size(p, vec2(2.0, cell.h)), 0.0, FG); }
        CursorShape::Underline => { painter.rect_filled(Rect::from_min_size(p + vec2(0.0, cell.h - 2.0), vec2(cell.w, 2.0)), 0.0, FG); }
        _ => {}
    }
}

// ---------- keys ----------

/// egui key → bytes for the PTY. None = not ours (text arrives via Event::Text).
pub fn key_bytes(key: Key, m: Modifiers, app_cursor: bool) -> Option<Cow<'static, [u8]>> {
    let s = |b: &'static str| Some(Cow::Borrowed(b.as_bytes()));
    let arrow = |c: char| {
        let pre = if app_cursor { "\x1bO" } else { "\x1b[" };
        Some(Cow::Owned(format!("{pre}{c}").into_bytes()))
    };
    if m.command {
        return None;
    }
    if m.ctrl {
        let name = key.name();
        if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() {
            return Some(Cow::Owned(vec![name.as_bytes()[0] - b'A' + 1]));
        }
    }
    match key {
        Key::Enter if m.shift => s("\x1b\r"),
        Key::Enter => s("\r"),
        Key::Tab if m.shift => s("\x1b[Z"),
        Key::Tab => s("\t"),
        Key::Backspace if m.alt => s("\x1b\x7f"),
        Key::Backspace => s("\x7f"),
        Key::Escape => s("\x1b"),
        Key::Delete => s("\x1b[3~"),
        Key::ArrowLeft if m.alt => s("\x1bb"),
        Key::ArrowRight if m.alt => s("\x1bf"),
        Key::ArrowUp => arrow('A'),
        Key::ArrowDown => arrow('B'),
        Key::ArrowRight => arrow('C'),
        Key::ArrowLeft => arrow('D'),
        Key::Home => s("\x1b[H"),
        Key::End => s("\x1b[F"),
        Key::PageUp => s("\x1b[5~"),
        Key::PageDown => s("\x1b[6~"),
        _ => None,
    }
}

pub fn grid_size(area: Vec2, cell: &Cell) -> (u16, u16) {
    ((area.x / cell.w).floor() as u16, (area.y / cell.h).floor() as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_colors() {
        let none = Modifiers::NONE;
        assert_eq!(key_bytes(Key::C, Modifiers::CTRL, false).unwrap().as_ref(), b"\x03");
        assert_eq!(key_bytes(Key::ArrowUp, none, true).unwrap().as_ref(), b"\x1bOA");
        assert_eq!(key_bytes(Key::ArrowUp, none, false).unwrap().as_ref(), b"\x1b[A");
        assert!(key_bytes(Key::T, Modifiers::COMMAND, false).is_none());
        assert!(key_bytes(Key::A, none, false).is_none());
        assert_eq!(session_name("✳ Fix login bug", "claude").as_deref(), Some("Fix login bug"));
        assert_eq!(session_name("⠂ Claude Code", "claude"), None);
        assert_eq!(session_name("", "codex"), None);
        assert_eq!(session_name("a very long session name that goes on", "grok").as_deref(), Some("a very long session name…"));
        assert_eq!(dim(Color32::WHITE), Color32::from_gray(170));
        assert_eq!(index_color(16), Color32::BLACK);
        assert_eq!(index_color(231), Color32::WHITE);
        assert_eq!(index_color(alacritty_terminal::vte::ansi::NamedColor::Background as usize), BG);
    }
}
