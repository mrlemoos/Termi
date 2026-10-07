use eframe::egui;

mod agent;
mod editor;
#[cfg(target_os = "macos")]
mod menu;
mod settings;
mod split;
mod term;
mod tree;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::{Color32, FontFamily, FontId, Key, Modifiers, Pos2, Sense, vec2};

use agent::State;
use editor::{Editor, Outcome};
use split::Layout;
use term::{BG, Cell, FG, Fonts, Tab};

const FONT_SIZE: f32 = 14.0;
/// Height of the hidden titlebar strip: hover shows traffic lights, drag moves the window.
const TITLEBAR: f32 = 28.0;

fn main() -> eframe::Result {
    alacritty_terminal::tty::setup_env();
    let _ = std::fs::create_dir_all(agent::state_dir());
    let viewport = egui::ViewportBuilder::default()
        .with_title("Termi")
        .with_inner_size([1000.0, 640.0])
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false);
    eframe::run_native(
        "Termi",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(|cc| Ok(Box::new(App::new(&cc.egui_ctx)))),
    )
}

struct App {
    tabs: Vec<Tab>,
    active: usize,
    /// One per screen; ⌘n shows the screen holding tab n.
    screens: Vec<Layout>,
    next_id: u64,
    tree: tree::Tree,
    show_tree: bool,
    /// Keys go to the tree instead of the terminal/editor.
    tree_focus: bool,
    editor: Option<Editor>,
    settings: settings::Settings,
    /// Some(cursor row) while the settings screen is open.
    settings_open: Option<usize>,
    /// Some(draft name) while renaming the active tab.
    renaming: Option<String>,
    fonts: Fonts,
    lights: Option<bool>,
    window_drag: bool,
    scroll_acc: f32,
    last_poll: Instant,
}

impl App {
    fn new(ctx: &egui::Context) -> App {
        install_fonts(ctx);
        #[cfg(target_os = "macos")]
        menu::install(ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = BG;
        visuals.extreme_bg_color = BG;
        visuals.override_text_color = Some(FG);
        ctx.set_visuals(visuals);

        let mut app = App {
            tabs: Vec::new(), active: 0, screens: Vec::new(), next_id: 1, tree: Default::default(), show_tree: false, tree_focus: false, editor: None,
            settings: settings::Settings::load(), settings_open: None, renaming: None,
            fonts: Fonts { regular: FontId::new(FONT_SIZE, FontFamily::Monospace), bold: FontId::new(FONT_SIZE, FontFamily::Name("bold".into())) },
            lights: None, window_drag: false, scroll_acc: 0.0, last_poll: Instant::now(),
        };
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into());
        app.new_tab(ctx, std::env::current_dir().ok().filter(|d| d != Path::new("/")).unwrap_or(home), None);
        app
    }

    /// `split`: Some(side_by_side) splits the active pane, None opens a new screen.
    fn new_tab(&mut self, ctx: &egui::Context, cwd: PathBuf, split: Option<bool>) {
        match Tab::spawn(self.next_id, cwd, ctx) {
            Ok(tab) => {
                match (split, self.tabs.get(self.active)) {
                    (Some(side), Some(at)) => {
                        let at = at.id;
                        self.screens = std::mem::take(&mut self.screens).into_iter().map(|l| l.split(at, tab.id, side)).collect();
                    }
                    _ => self.screens.push(Layout::Leaf(tab.id)),
                }
                self.tabs.push(tab);
                self.active = self.tabs.len() - 1;
                self.next_id += 1;
            }
            Err(e) => eprintln!("termi: failed to spawn shell: {e}"),
        }
    }

    fn cell(&self, ctx: &egui::Context) -> Cell {
        ctx.fonts_mut(|f| Cell { w: f.glyph_width(&self.fonts.regular, 'M'), h: f.row_height(&self.fonts.regular) })
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        // renaming swallows every key until Enter/Escape
        if let Some(buf) = &mut self.renaming {
            let events = ctx.input_mut(|i| std::mem::take(&mut i.events));
            match events.iter().find_map(|ev| rename_key(buf, ev)) {
                Some(true) => {
                    let name = buf.trim().to_owned();
                    self.tabs[self.active].name = Some(name).filter(|n| !n.is_empty());
                    self.renaming = None;
                }
                Some(false) => self.renaming = None,
                None => {}
            }
            return;
        }
        let cmd = |k| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, k));
        if cmd(Key::R) {
            self.renaming = Some(self.tabs[self.active].label());
        }
        // ⌘⇧\ before ⌘\: consume_key ignores extra shift
        let split = if cmd(Key::Pipe) || ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Backslash)) {
            Some(Some(false))
        } else if cmd(Key::Backslash) {
            Some(Some(true))
        } else if cmd(Key::T) {
            Some(None)
        } else {
            None
        };
        if let Some(split) = split {
            let cwd = self.tabs[self.active].cwd.clone();
            self.new_tab(ctx, cwd, split);
        }
        if cmd(Key::W) {
            self.tabs.remove(self.active);
        }
        if cmd(Key::Comma) {
            self.settings_open = if self.settings_open.is_some() { None } else { Some(0) };
        }
        if cmd(Key::B) {
            self.show_tree = !self.show_tree;
            self.tree_focus = self.show_tree;
        }
        if self.show_tree && cmd(Key::ArrowRight) {
            self.tree_focus = true;
        }
        if self.tree_focus && cmd(Key::ArrowLeft) {
            self.tree_focus = false;
        }
        let nums = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
        for (i, k) in nums.into_iter().enumerate() {
            if cmd(k) && i < self.tabs.len() {
                self.active = i;
            }
        }
    }
}

/// One key into the rename draft: Some(true) commit, Some(false) cancel.
fn rename_key(buf: &mut String, ev: &egui::Event) -> Option<bool> {
    match ev {
        egui::Event::Text(t) => buf.push_str(t),
        egui::Event::Paste(t) => buf.push_str(t.lines().next().unwrap_or("")),
        egui::Event::Key { key: Key::Backspace, pressed: true, .. } => _ = buf.pop(),
        egui::Event::Key { key: Key::Enter, pressed: true, .. } => return Some(true),
        egui::Event::Key { key: Key::Escape, pressed: true, .. } => return Some(false),
        _ => {}
    }
    None
}

fn install_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::default();
    let font = |b: &'static [u8]| Arc::new(egui::FontData::from_static(b));
    defs.font_data.insert("meslo".into(), font(include_bytes!("../assets/MesloLGSNerdFontMono-Regular.ttf")));
    defs.font_data.insert("meslo-bold".into(), font(include_bytes!("../assets/MesloLGSNerdFontMono-Bold.ttf")));
    // Everything is monospace: it's a terminal.
    for fam in [FontFamily::Monospace, FontFamily::Proportional] {
        defs.families.entry(fam).or_default().insert(0, "meslo".into());
    }
    defs.families.insert(FontFamily::Name("bold".into()), vec!["meslo-bold".into(), "meslo".into()]);
    // Except tab titles: the system font (SF), like iTerm, semibold at its small-text optical size.
    let mut tab = vec!["meslo".into()];
    if let Ok(b) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        let tweak = egui::FontTweak { coords: egui::epaint::text::VariationCoords::new([("wght", 600.0), ("opsz", 17.0)]), ..Default::default() };
        defs.font_data.insert("sf".into(), Arc::new(egui::FontData::from_owned(b).tweak(tweak)));
        tab.insert(0, "sf".into());
    }
    defs.families.insert(FontFamily::Name("tab".into()), tab);
    ctx.set_fonts(defs);
}

#[cfg(target_os = "macos")]
fn set_traffic_lights(show: bool) {
    use objc2_app_kit::{NSApplication, NSWindowButton};
    let Some(mtm) = objc2::MainThreadMarker::new() else { return };
    for w in NSApplication::sharedApplication(mtm).windows().iter() {
        for b in [NSWindowButton::CloseButton, NSWindowButton::MiniaturizeButton, NSWindowButton::ZoomButton] {
            if let Some(btn) = w.standardWindowButton(b) {
                btn.setHidden(!show);
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn set_traffic_lights(_: bool) {}

/// Path as typed into the prompt: relative to the tab's cwd, agent-specific format.
fn file_ref(tab: &Tab, path: &Path) -> String {
    let rel = path.strip_prefix(&tab.cwd).unwrap_or(path).to_string_lossy();
    match tab.agent {
        Some(a) => a.file_ref(&rel),
        None => agent::shell_quote(&rel),
    }
}

fn tilde(p: &Path) -> String {
    let s = p.to_string_lossy();
    match std::env::var("HOME") {
        Ok(h) if s.starts_with(&h) => format!("~{}", &s[h.len()..]),
        _ => s.into_owned(),
    }
}

/// Grey ↔ white, 1.2s period: "this tab is working".
fn pulse() -> Color32 {
    let p = (epoch_ms() as f32 / 1200.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5;
    Color32::from_gray((110.0 + 145.0 * p) as u8)
}

fn epoch_ms() -> usize {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as usize)
}

impl eframe::App for App {
    /// Unpainted areas are the terminal background, not eframe's default grey.
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        BG.to_normalized_gamma_f32()
    }

    #[cfg(target_os = "macos")]
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        raw.events.extend(menu::take());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        for t in &mut self.tabs {
            t.pump(&ctx);
        }
        self.tabs.retain(|t| !t.exited);
        if self.tabs.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.shortcuts(&ctx);
        if self.tabs.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        let alive: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
        self.screens = std::mem::take(&mut self.screens).into_iter().filter_map(|l| l.retain(&|id| alive.contains(&id))).collect();
        if self.last_poll.elapsed() > Duration::from_secs(1) {
            let before: Vec<(State, bool)> = self.tabs.iter().map(|t| (t.state, t.done)).collect();
            self.tabs.iter_mut().for_each(Tab::poll);
            let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
            for (i, (tab, (state, done))) in self.tabs.iter().zip(before).enumerate() {
                if !self.settings.notifications || (focused && i == self.active) {
                    continue;
                }
                if tab.state == State::NeedsInput && state != State::NeedsInput {
                    settings::notify(&tab.label(), "needs your input");
                } else if tab.done && !done {
                    settings::notify(&tab.label(), "done");
                }
            }
            self.last_poll = Instant::now();
        }

        // you're looking at the active tab, so its "done" has been seen
        if ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
            self.tabs[self.active].done = false;
        }

        let show = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| p.y < TITLEBAR);
        if self.lights != Some(show) {
            set_traffic_lights(show);
            self.lights = Some(show);
        }

        let cell = self.cell(&ctx);
        let font = self.fonts.regular.clone();

        // ---- status line: ⌘n badges, tmux style ----
        egui::Panel::bottom("status").exact_size(cell.h).show_separator_line(false).frame(egui::Frame::NONE.fill(BG)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.horizontal(|ui| {
                for (i, tab) in self.tabs.iter().enumerate() {
                    let glyph = match (tab.agent.is_some(), tab.state, tab.done) {
                        (true, State::NeedsInput, _) => "! ".into(),
                        (true, State::Idle, true) => "✓ ".into(),
                        _ => String::new(),
                    };
                    // first badge: pad inside it so its fill reaches the window's rounded corner but ⌘ clears it
                    let pad = if i == 0 { "    " } else { " " };
                    let label = match &self.renaming {
                        Some(buf) if i == self.active => format!("{buf}█"),
                        _ => tab.label(),
                    };
                    let mut text = egui::RichText::new(format!("{pad}⌘{} {glyph}{label} ", i + 1)).font(FontId::new(FONT_SIZE + 1.0, FontFamily::Name("tab".into())));
                    text = match (i == self.active, tab.state) {
                        (true, State::Working) if tab.agent.is_some() => text.color(BG).background_color(pulse()),
                        (true, _) => text.color(BG).background_color(FG),
                        (false, State::Working) if tab.agent.is_some() => text.color(pulse()),
                        (false, State::NeedsInput) => text.color(Color32::from_rgb(0xcd, 0xcd, 0x00)),
                        (false, State::Idle) if tab.done => text.color(Color32::from_rgb(0x00, 0xcd, 0x00)),
                        _ => text.color(Color32::from_gray(160)),
                    };
                    let resp = ui.add(egui::Label::new(text).sense(Sense::click()).selectable(false));
                    if resp.clicked() {
                        self.active = i;
                    }
                    if resp.double_clicked() {
                        self.renaming = Some(tab.label());
                    }
                    if let Some(a) = tab.agent {
                        let cols = a.sprite(0).iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
                        let (r, _) = ui.allocate_exact_size(vec2((cols / 3.0 + 1.0) * cell.w, cell.h), Sense::hover());
                        mascot(ui.painter(), r.min + vec2(cell.w * 0.5, 0.0), &cell, a, tab.state);
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // clear the window's rounded corner
                    ui.add_space(cell.w * 2.0);
                    ui.label(egui::RichText::new(format!("{}", tilde(&self.tabs[self.active].cwd))).font(font.clone()).color(Color32::from_gray(120)));
                });
            });
        });

        // ---- tree ----
        if self.show_tree {
            let cwd = self.tabs[self.active].cwd.clone();
            egui::Panel::left("tree")
                .resizable(true)
                .default_size(260.0)
                .frame(egui::Frame::NONE.fill(BG).inner_margin(egui::Margin { left: 6, right: 6, top: TITLEBAR as i8, bottom: 0 }))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        if let Some(tree::Action::Open(p)) = self.tree.show(ui, &cwd, &font, &mut self.tree_focus) {
                            match Editor::open(p) {
                                Ok(ed) => self.editor = Some(ed),
                                Err(e) => eprintln!("termi: {e}"),
                            }
                        }
                    });
                });
        }

        let central = egui::CentralPanel::no_frame().show(ui, |ui| {
            if let Some(cursor) = &mut self.settings_open {
                ui.add_space(TITLEBAR);
                if !self.settings.show(ui, &font, cursor) {
                    self.settings_open = None;
                }
                return;
            }
            if let Some(ed) = &mut self.editor {
                let tab = &self.tabs[self.active];
                let shown = ed.path.strip_prefix(&tab.cwd).unwrap_or(&ed.path).to_string_lossy().into_owned();
                ui.add_space(TITLEBAR);
                match ed.show(ui, &font, &shown) {
                    Outcome::Stay => {}
                    Outcome::Close => self.editor = None,
                    Outcome::Send(prompt) => {
                        tab.paste(&prompt);
                        self.editor = None;
                    }
                }
                return;
            }
            let area = ui.available_rect_before_wrap();
            let active = self.tabs[self.active].id;
            let screen = self.screens.iter().find(|l| l.contains(active)).cloned().unwrap_or(Layout::Leaf(active));
            let panes = screen.rects(area, (cell.w, cell.h));
            let ids: Vec<egui::Id> = panes.iter().map(|&(id, _)| pane_id(id)).collect();
            for &(id, r) in &panes {
                let Some(i) = self.tabs.iter().position(|t| t.id == id) else { continue };
                self.terminal(ui, &ctx, &cell, show, i, r, r.min.y <= area.min.y, &ids);
                // divider on the shared edge, a box-drawing hairline
                let line = egui::Stroke::new(1.0, Color32::from_gray(70));
                if r.max.x < area.max.x {
                    ui.painter().vline(r.max.x, r.y_range(), line);
                }
                if r.max.y < area.max.y {
                    ui.painter().hline(r.x_range(), r.max.y, line);
                }
            }
        });
        if central.response.contains_pointer() && ctx.input(|i| i.pointer.any_pressed()) {
            self.tree_focus = false;
        }

        // drag preview for tree → prompt
        if let (Some(path), Some(pos)) = (egui::DragAndDrop::payload::<PathBuf>(&ctx), ctx.pointer_hover_pos()) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("dnd")));
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let r = painter.text(pos + vec2(12.0, 4.0), egui::Align2::LEFT_TOP, format!(" {name}"), font.clone(), BG);
            painter.rect_filled(r.expand(2.0), 0.0, FG);
            painter.text(pos + vec2(12.0, 4.0), egui::Align2::LEFT_TOP, format!(" {name}"), font.clone(), BG);
        }

        let busy = self.tabs.iter().any(|t| t.agent.is_some() && t.state == State::Working);
        ctx.request_repaint_after(Duration::from_millis(if busy { 33 } else { 1000 }));
    }
}

impl App {
    /// One pane: tab `i` in `rect`. `top` panes leave room for the titlebar; `panes` are the screen's pane ids.
    fn terminal(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, cell: &Cell, lights: bool, i: usize, rect: egui::Rect, top: bool, panes: &[egui::Id]) {
        let resp = ui.interact(rect, pane_id(self.tabs[i].id), Sense::click_and_drag());
        if resp.clicked() || resp.drag_started() {
            self.active = i;
        }
        let pad = if top { TITLEBAR } else { 4.0 };
        let origin = rect.min + vec2(4.0, pad);
        let is_active = i == self.active;
        let tab = &mut self.tabs[i];
        let (cols, rows) = term::grid_size(rect.size() - vec2(8.0, pad), cell);
        tab.resize(cols, rows, cell);

        // focus follows the active pane, unless something else (not a pane) holds it
        if is_active && (resp.clicked() || ui.memory(|m| m.focused().is_none_or(|f| f != resp.id && panes.contains(&f)))) {
            resp.request_focus();
        }
        let filter = egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true };
        ui.memory_mut(|m| m.set_focus_lock_filter(resp.id, filter));

        if resp.has_focus() {
            let app_cursor = tab.term.lock().mode().contains(alacritty_terminal::term::TermMode::APP_CURSOR);
            for ev in ctx.input(|i| i.events.clone()) {
                match ev {
                    egui::Event::Text(s) => tab.paste_raw(&s),
                    egui::Event::Key { key, pressed: true, modifiers, .. } => {
                        if let Some(b) = term::key_bytes(key, modifiers, app_cursor) {
                            tab.paste_raw(std::str::from_utf8(&b).unwrap_or(""));
                        }
                    }
                    egui::Event::Paste(s) => tab.paste(&s),
                    egui::Event::Copy => {
                        if let Some(s) = tab.term.lock().selection_to_string() {
                            ctx.copy_text(s);
                        }
                    }
                    _ => {}
                }
            }
        }
        if resp.hovered() {
            for ev in ctx.input(|i| i.events.clone()) {
                if let egui::Event::MouseWheel { unit, delta, .. } = ev {
                    self.scroll_acc += match unit {
                        egui::MouseWheelUnit::Point => delta.y / cell.h,
                        egui::MouseWheelUnit::Line => delta.y,
                        egui::MouseWheelUnit::Page => delta.y * rows as f32,
                    };
                }
            }
            let lines = self.scroll_acc.trunc() as i32;
            if lines != 0 {
                tab.scroll(lines);
                self.scroll_acc -= lines as f32;
            }
        }

        // mouse: titlebar strip drags the window, elsewhere selects text
        if resp.drag_started() {
            let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or(rect.min);
            self.window_drag = lights && start.y < TITLEBAR;
            if self.window_drag {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            } else {
                tab.start_selection(start, origin, cell);
            }
        }
        if resp.dragged() && !self.window_drag {
            if let Some(p) = resp.interact_pointer_pos() {
                tab.update_selection(p, origin, cell);
            }
        }
        if resp.clicked() {
            tab.term.lock().selection = None;
        }

        // drops: tree rows and Finder files
        if let Some(path) = resp.dnd_release_payload::<PathBuf>() {
            tab.paste_raw(&file_ref(tab, &path));
        }
        if is_active {
            for f in ctx.input(|i| i.raw.dropped_files.clone()) {
                tab.paste_raw(&file_ref(tab, f.path()));
            }
        }

        let painter = ui.painter_at(rect);
        // hollow cursor while the tree has the keys
        term::paint(tab, &painter, origin, cell, &self.fonts, resp.has_focus() && !self.tree_focus);
        if resp.dnd_hover_payload::<PathBuf>().is_some() {
            painter.rect_stroke(rect.shrink(1.0), 0.0, egui::Stroke::new(1.0, FG), egui::StrokeKind::Inside);
        }
    }
}

fn pane_id(tab: u64) -> egui::Id {
    egui::Id::new(("pane", tab))
}

/// Tamagotchi: the 3-row sprite squeezed into one status-line row. Walks while working.
fn mascot(painter: &egui::Painter, top_left: Pos2, cell: &Cell, a: &dyn agent::Agent, state: State) {
    let font = FontId::new(FONT_SIZE / 3.0, FontFamily::Monospace);
    let frame = if state == State::Working { epoch_ms() / 250 } else { 0 };
    let [r, g, b] = a.color();
    for (i, line) in a.sprite(frame).iter().enumerate() {
        let pos = top_left + vec2(0.0, i as f32 * cell.h / 3.0);
        painter.text(pos, egui::Align2::LEFT_TOP, *line, font.clone(), Color32::from_rgb(r, g, b));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_key_edits_commits_cancels() {
        let key = |key| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        let mut buf = "zsh".to_owned();
        assert_eq!(rename_key(&mut buf, &key(Key::Backspace)), None);
        assert_eq!(rename_key(&mut buf, &egui::Event::Text("!".into())), None);
        assert_eq!(buf, "zs!");
        assert_eq!(rename_key(&mut buf, &key(Key::Enter)), Some(true));
        assert_eq!(rename_key(&mut buf, &key(Key::Escape)), Some(false));
    }
}
