use eframe::egui;

mod agent;
mod glass;
mod editor;
mod markdown;
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
use term::{Cell, Fonts, Tab, background, foreground};

/// Height of the hidden titlebar strip: hover shows traffic lights, drag moves the window.
const TITLEBAR: f32 = 40.0;

fn main() -> eframe::Result {
    alacritty_terminal::tty::setup_env();
    let _ = std::fs::create_dir_all(agent::state_dir());
    let viewport = egui::ViewportBuilder::default()
        .with_title("Termi")
        .with_transparent(true)
        .with_inner_size([1000.0, 640.0])
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        // eframe sets the Dock icon at runtime (egui logo by default), overriding the bundle's Termi.icns.
        .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/termi-icon.png")).expect("valid icon png"));
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
    /// Tab id of the pane being ⌘-dragged.
    pane_drag: Option<u64>,
    lights: Option<bool>,
    #[cfg(target_os = "macos")]
    blur: Option<objc2::rc::Retained<objc2_app_kit::NSVisualEffectView>>,
    window_drag: bool,
    scroll_acc: f32,
    last_poll: Instant,
}

impl App {
    fn new(ctx: &egui::Context) -> App {
        #[cfg(target_os = "macos")]
        menu::install(ctx);
        let settings = settings::Settings::load();
        apply(ctx, &settings);
        let mut app = App {
            tabs: Vec::new(), active: 0, screens: Vec::new(), next_id: 1, tree: Default::default(), show_tree: false, tree_focus: false, editor: None,
            settings, settings_open: None, renaming: None, pane_drag: None,
            lights: None, window_drag: false, scroll_acc: 0.0, last_poll: Instant::now(),
            #[cfg(target_os = "macos")]
            blur: None,
        };
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into());
        app.new_tab(ctx, std::env::current_dir().ok().filter(|d| d != Path::new("/")).unwrap_or(home), None);
        app
    }

    /// `split`: Some(side_by_side) splits the active pane, None opens a new screen.
    fn new_tab(&mut self, ctx: &egui::Context, cwd: PathBuf, split: Option<bool>) {
        match Tab::spawn(self.next_id, cwd, &self.settings.shell, ctx) {
            Ok(tab) => {
                match (split, self.tabs.get(self.active)) {
                    (Some(side), Some(at)) => {
                        let at = at.id;
                        self.screens = std::mem::take(&mut self.screens).into_iter().map(|l| l.split(at, tab.id, side, false)).collect();
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

    fn fonts(&self) -> Fonts {
        let size = self.settings.font_size as f32;
        Fonts { regular: FontId::new(size, FontFamily::Monospace), bold: FontId::new(size, FontFamily::Name("bold".into())) }
    }

    fn cell(&self, ctx: &egui::Context) -> Cell {
        ctx.fonts_mut(|f| Cell { w: f.glyph_width(&self.fonts().regular, 'M'), h: f.row_height(&self.fonts().regular) })
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

/// Theme, colours and fonts from the settings. Font size isn't here: it's read every frame.
fn apply(ctx: &egui::Context, s: &settings::Settings) {
    term::set_theme(&s.theme);
    let mut visuals = if term::theme().light { egui::Visuals::light() } else { egui::Visuals::dark() };
    visuals.panel_fill = if s.glass == "off" { background() } else { Color32::TRANSPARENT };
    visuals.window_fill = background();
    visuals.extreme_bg_color = background();
    visuals.override_text_color = Some(foreground());
    ctx.set_visuals(visuals);
    install_fonts(ctx, &s.font);
}

/// `name` from `settings::FONTS`; Meslo backs it up for missing files and glyphs (it has the Nerd Font icons).
fn install_fonts(ctx: &egui::Context, name: &str) {
    let mut defs = egui::FontDefinitions::default();
    let font = |b: &'static [u8]| Arc::new(egui::FontData::from_static(b));
    defs.font_data.insert("meslo".into(), font(include_bytes!("../assets/MesloLGSNerdFontMono-Regular.ttf")));
    defs.font_data.insert("meslo-bold".into(), font(include_bytes!("../assets/MesloLGSNerdFontMono-Bold.ttf")));
    let (mut mono, mut bold) = (vec!["meslo".to_owned()], vec!["meslo-bold".to_owned(), "meslo".to_owned()]);
    let file = |path: &str, index: u32| std::fs::read(path).ok().map(|b| Arc::new(egui::FontData { index, ..egui::FontData::from_owned(b) }));
    if let Some((_, Some((r, ri, b, bi)))) = settings::FONTS.iter().find(|(n, _)| *n == name) {
        if let (Some(r), Some(b)) = (file(r, *ri), file(b, *bi)) {
            defs.font_data.insert("user".into(), r);
            defs.font_data.insert("user-bold".into(), b);
            mono.insert(0, "user".into());
            bold.insert(0, "user-bold".into());
        }
    }
    // Everything is monospace: it's a terminal.
    for fam in [FontFamily::Monospace, FontFamily::Proportional] {
        let list = defs.families.entry(fam).or_default();
        for (i, f) in mono.iter().enumerate() {
            list.insert(i, f.clone());
        }
    }
    defs.families.insert(FontFamily::Name("bold".into()), bold);
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
        if self.settings.glass == "off" { background().to_normalized_gamma_f32() } else { [0.0; 4] }
    }

    #[cfg(target_os = "macos")]
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        raw.events.extend(menu::take());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let glass = self.settings.glass != "off";
        #[cfg(target_os = "macos")]
        glass::blur(&mut self.blur, glass);
        if glass {
            let time = if self.settings.glass == "wave" { ctx.input(|i| i.time) } else { 0.0 };
            glass::paint(ui.painter(), ui.max_rect(), time, term::theme().light);
            if self.settings.glass == "wave" && ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
                ctx.request_repaint_after(Duration::from_millis(33));
            }
        }
        let panel_fill = if glass { Color32::TRANSPARENT } else { background() };
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
        let font = self.fonts().regular;

        // ---- status line: ⌘n badges, tmux style ----
        egui::Panel::bottom("status").exact_size(cell.h + 12.0).show_separator_line(false).frame(egui::Frame::NONE.fill(panel_fill).inner_margin(egui::Margin { left: 0, right: 0, top: 6, bottom: 6 })).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let bottom = ui.max_rect().bottom();
            ui.horizontal(|ui| {
                for (i, tab) in self.tabs.iter().enumerate() {
                    let glyph = match (tab.agent.is_some(), tab.state, tab.done) {
                        (true, State::NeedsInput, _) => "! ".into(),
                        (true, State::Idle, true) => "✓ ".into(),
                        _ => String::new(),
                    };
                    let label = match &self.renaming {
                        Some(buf) if i == self.active => format!("{buf}█"),
                        _ => tab.label(),
                    };
                    let mut text = egui::RichText::new(format!("    ⌘{} {glyph}{label}    ", i + 1)).font(FontId::new(font.size + 1.0, FontFamily::Name("tab".into())));
                    let fill = match (i == self.active, tab.state) {
                        (true, State::Working) if tab.agent.is_some() => Some(pulse()),
                        (true, _) => Some(foreground()),
                        _ => None,
                    };
                    text = match (i == self.active, tab.state) {
                        (true, _) => text.color(background()),
                        (false, State::Working) if tab.agent.is_some() => text.color(pulse()),
                        (false, State::NeedsInput) => text.color(Color32::from_rgb(0xcd, 0xcd, 0x00)),
                        (false, State::Idle) if tab.done => text.color(Color32::from_rgb(0x00, 0xcd, 0x00)),
                        _ => text.color(Color32::from_gray(160)),
                    };
                    // badge fill runs down to the window's bottom edge, painted under the label
                    let bg = ui.painter().add(egui::Shape::Noop);
                    let resp = ui.add(egui::Label::new(text).sense(Sense::click()).selectable(false));
                    if let Some(c) = fill {
                        ui.painter().set(bg, egui::Shape::rect_filled(resp.rect.with_min_y(resp.rect.top() - 6.0).with_max_y(bottom + 6.0), 0.0, c));
                    }
                    if resp.clicked() {
                        self.active = i;
                    }
                    if resp.double_clicked() {
                        self.renaming = Some(tab.label());
                    }
                    if let Some(a) = tab.agent {
                        let cols = a.sprite(0).iter().map(|l| l.chars().count()).max().unwrap_or(0) as f32;
                        let (r, _) = ui.allocate_exact_size(vec2((cols / 3.0 + 1.0) * cell.w, cell.h), Sense::hover());
                        mascot(ui.painter(), r.min + vec2(cell.w * 0.5, 0.0), &cell, font.size, a, tab.state);
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
                .frame(egui::Frame::NONE.fill(panel_fill).inner_margin(egui::Margin { left: 6, right: 6, top: TITLEBAR as i8, bottom: 0 }))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        if let Some(tree::Action::Open(p)) = self.tree.show(ui, &cwd, &font, &mut self.tree_focus) {
                            if let Some(ed) = &mut self.editor {
                                ed.navigate(p);
                                return;
                            }
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
                let before = self.settings.clone();
                if !self.settings.show(ui, &font, cursor) {
                    self.settings_open = None;
                }
                if self.settings != before {
                    apply(&ctx, &self.settings);
                    self.settings.save();
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
            let si = self.screens.iter().position(|l| l.contains(active));
            let screen = si.map_or(Layout::Leaf(active), |si| self.screens[si].clone());
            let snap = (cell.w, cell.h);
            let panes = screen.rects(area, snap);
            let ids: Vec<egui::Id> = panes.iter().map(|&(id, _)| pane_id(id)).collect();
            for &(id, r) in &panes {
                let Some(i) = self.tabs.iter().position(|t| t.id == id) else { continue };
                self.terminal(ui, &ctx, &cell, show, i, r, r.min.y <= area.min.y, &ids);
            }
            // ⌘ held: the whole screen grabs the mouse; drag a pane onto another's edge to re-split it there.
            // Without ⌘ the panes keep the mouse for text selection.
            let grab_id = egui::Id::new("pane-grab");
            let under = |p: Option<Pos2>| p.and_then(|p| panes.iter().find(|(_, r)| r.contains(p)).copied());
            if panes.len() > 1 && (ctx.input(|i| i.modifiers.command) || ctx.is_being_dragged(grab_id)) {
                let resp = ui.interact(area, grab_id, Sense::drag());
                let target = under(ctx.pointer_hover_pos());
                if resp.drag_started() {
                    self.pane_drag = under(ctx.input(|i| i.pointer.press_origin())).map(|(id, _)| id);
                }
                let outline = |r: egui::Rect, c| ui.painter().rect_stroke(r.shrink(1.0), 0.0, egui::Stroke::new(2.0, c), egui::StrokeKind::Inside);
                match self.pane_drag.filter(|_| resp.dragged() || resp.drag_stopped()) {
                    Some(src) => {
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                        if let Some((_, r)) = panes.iter().find(|(id, _)| *id == src) {
                            outline(*r, Color32::from_gray(90));
                        }
                        if let (Some((dst, r)), Some(p)) = (target.filter(|(dst, _)| *dst != src), ctx.pointer_hover_pos()) {
                            // the edge of the target nearest the pointer
                            let d = (p - r.center()) / r.size();
                            let (side, before) = if d.x.abs() > d.y.abs() { (true, d.x < 0.0) } else { (false, d.y < 0.0) };
                            let after = screen.clone().moved(src, dst, side, before);
                            // preview where src lands, in terminal bright blue
                            if let Some((_, land)) = after.rects(area, snap).into_iter().find(|(id, _)| *id == src) {
                                ui.painter().rect_filled(land, 0.0, Color32::from_rgba_unmultiplied(0x5c, 0x5c, 0xff, 40));
                                outline(land, Color32::from_rgb(0x5c, 0x5c, 0xff));
                            }
                            if let (true, Some(si)) = (resp.drag_stopped(), si) {
                                self.screens[si] = after;
                            }
                        }
                        if resp.drag_stopped() {
                            self.pane_drag = None;
                        }
                    }
                    None => {
                        ctx.set_cursor_icon(egui::CursorIcon::Grab);
                        if let Some((_, r)) = target {
                            outline(r, foreground());
                        }
                    }
                }
            }
            for d in screen.dividers(area, snap) {
                ui.painter().line_segment(d, egui::Stroke::new(1.0, Color32::from_gray(70)));
            }
        });
        if central.response.contains_pointer() && ctx.input(|i| i.pointer.any_pressed()) {
            self.tree_focus = false;
        }

        // drag preview for tree → prompt
        if let (Some(path), Some(pos)) = (egui::DragAndDrop::payload::<PathBuf>(&ctx), ctx.pointer_hover_pos()) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("dnd")));
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let r = painter.text(pos + vec2(12.0, 4.0), egui::Align2::LEFT_TOP, format!(" {name}"), font.clone(), background());
            painter.rect_filled(r.expand(2.0), 0.0, foreground());
            painter.text(pos + vec2(12.0, 4.0), egui::Align2::LEFT_TOP, format!(" {name}"), font.clone(), background());
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
        let fonts = self.fonts();
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
        term::paint(tab, &painter, origin, cell, &fonts, resp.has_focus() && !self.tree_focus);
        if resp.dnd_hover_payload::<PathBuf>().is_some() {
            painter.rect_stroke(rect.shrink(1.0), 0.0, egui::Stroke::new(1.0, foreground()), egui::StrokeKind::Inside);
        }
    }
}

fn pane_id(tab: u64) -> egui::Id {
    egui::Id::new(("pane", tab))
}

/// Tamagotchi: the 3-row sprite squeezed into one status-line row. Walks while working.
fn mascot(painter: &egui::Painter, top_left: Pos2, cell: &Cell, size: f32, a: &dyn agent::Agent, state: State) {
    let font = FontId::new(size / 3.0, FontFamily::Monospace);
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
        // a multi-line paste only keeps its first line
        assert_eq!(rename_key(&mut buf, &egui::Event::Paste("ab\ncd".into())), None);
        assert_eq!(buf, "zs!ab");
        let release = egui::Event::Key { key: Key::Enter, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE };
        assert_eq!(rename_key(&mut buf, &release), None);
    }

    #[test]
    fn tilde_shortens_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(tilde(&Path::new(&home).join("src")), "~/src");
        assert_eq!(tilde(Path::new("/tmp")), "/tmp");
    }

    #[test]
    fn pane_ids_differ_per_tab() {
        assert_ne!(pane_id(1), pane_id(2));
        assert_eq!(pane_id(1), pane_id(1));
    }

    /// Visual tests: the ⌘, screen rendered in every theme and font. `UPDATE_SNAPSHOTS=1 cargo test` to re-record.
    fn settings_screen(s: settings::Settings, cursor: usize) -> egui_kittest::Harness<'static, settings::Settings> {
        let mut h = egui_kittest::Harness::builder().with_size(vec2(720.0, 260.0)).build_ui_state(
            move |ui, s: &mut settings::Settings| {
                let before = s.clone();
                ui.painter().rect_filled(ui.max_rect(), 0.0, background());
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                let mut c = cursor;
                s.show(ui, &FontId::monospace(s.font_size as f32), &mut c);
                if *s != before {
                    apply(ui.ctx(), s);
                }
            },
            s,
        );
        apply(&h.ctx, h.state());
        h.run();
        h
    }

    #[test]
    fn settings_snapshots() {
        let _theme = term::THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut results = egui_kittest::SnapshotResults::new();
        let mut shot = |s: settings::Settings, cursor: usize, name: &str| {
            let mut h = settings_screen(s, cursor);
            h.snapshot(name);
            results.extend_harness(&mut h);
        };
        let base = settings::Settings::default();
        for t in &term::THEMES {
            shot(settings::Settings { theme: t.name.into(), ..base.clone() }, 3, &format!("settings_theme_{}", t.name.replace(' ', "_")));
        }
        for (name, _) in settings::FONTS {
            shot(settings::Settings { font: name.into(), ..base.clone() }, 2, &format!("settings_font_{}", name.replace(' ', "_")));
        }
        shot(settings::Settings { font_size: 20, ..base.clone() }, 1, "settings_font_size_20");

        // → on the colours row switches theme and repaints in it
        let mut h = settings_screen(base.clone(), 3);
        h.key_press(Key::ArrowRight);
        h.run();
        assert_eq!(h.state().theme, "solarized dark");
        assert_eq!(background(), Color32::from_rgb(0x00, 0x2b, 0x36));
        h.snapshot("settings_after_right_arrow");
        results.extend_harness(&mut h);
        term::set_theme("xterm");
        results.unwrap();
    }
}
