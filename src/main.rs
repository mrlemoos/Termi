use eframe::egui;

mod agent;
mod editor;
mod glass;
mod markdown;
#[cfg(target_os = "macos")]
mod menu;
#[cfg(target_os = "macos")]
mod notifications;
mod session;
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
        .with_icon(
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/termi-icon.png"))
                .expect("valid icon png"),
        );
    eframe::run_native(
        "Termi",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
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
    scroll_acc: f32,
    last_poll: Instant,
    /// Cursor on the first-launch choice.
    welcome: usize,
    /// Last session text written to disk.
    session_saved: String,
}

impl App {
    fn new(ctx: &egui::Context) -> App {
        let settings = settings::Settings::load();
        apply(ctx, &settings);
        let mut app = App {
            tabs: Vec::new(),
            active: 0,
            screens: Vec::new(),
            next_id: 1,
            tree: Default::default(),
            show_tree: false,
            tree_focus: false,
            editor: None,
            settings,
            settings_open: None,
            renaming: None,
            pane_drag: None,
            lights: None,
            scroll_acc: 0.0,
            last_poll: Instant::now(),
            welcome: 0,
            session_saved: String::new(),
            #[cfg(target_os = "macos")]
            blur: None,
        };
        if app.settings.resume == Some(true) {
            if let Some(snap) = session::load().and_then(|s| session::openable(s, |p| p.is_dir())) {
                app.restore(ctx, snap);
            }
        }
        if app.tabs.is_empty() && app.settings.resume.is_some() {
            app.open_initial(ctx);
        }
        if !app.tabs.is_empty() {
            app.session_saved = session::serialize(&app.snapshot());
        }
        #[cfg(target_os = "macos")]
        {
            notifications::install();
            menu::install(ctx);
        }
        app
    }

    /// `split`: Some(side_by_side) splits the active pane, None opens a new screen.
    fn new_tab(&mut self, ctx: &egui::Context, cwd: PathBuf, split: Option<bool>) {
        match Tab::spawn(self.next_id, cwd, &self.settings.shell, ctx) {
            Ok(tab) => {
                let splitting = match (split, self.tabs.get(self.active).map(|t| t.id)) {
                    (Some(side), Some(at)) => {
                        self.screens = std::mem::take(&mut self.screens)
                            .into_iter()
                            .map(|l| l.split(at, tab.id, side, false))
                            .collect();
                        true
                    }
                    _ => {
                        self.screens.push(Layout::Leaf(tab.id));
                        false
                    }
                };
                let at = tab_slot(self.active, self.tabs.len(), splitting);
                self.tabs.insert(at, tab);
                self.active = at;
                self.next_id += 1;
            }
            Err(e) => eprintln!("termi: failed to spawn shell: {e}"),
        }
    }

    fn open_initial(&mut self, ctx: &egui::Context) {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/".into());
        let cwd = std::env::current_dir()
            .ok()
            .filter(|d| d != Path::new("/"))
            .unwrap_or(home);
        self.new_tab(ctx, cwd, None);
    }

    fn choose_launch(&mut self, pick_up: bool, ctx: &egui::Context) {
        self.settings.resume = Some(pick_up);
        self.settings.save();
        if pick_up {
            if let Some(snap) = session::load().and_then(|s| session::openable(s, |p| p.is_dir())) {
                self.restore(ctx, snap);
            }
        }
        if self.tabs.is_empty() {
            self.open_initial(ctx);
        }
        self.session_saved = session::serialize(&self.snapshot());
    }

    fn restore(&mut self, ctx: &egui::Context, snap: session::Snapshot) {
        for tab in snap.tabs {
            let _ = std::fs::remove_file(agent::state_dir().join(tab.id.to_string()));
            match Tab::spawn(tab.id, tab.cwd, &self.settings.shell, ctx) {
                Ok(mut spawned) => {
                    spawned.name = tab.name;
                    if let Some(name) = tab.agent.clone() {
                        spawned.held_agent = Some(name);
                        spawned.held_since = Some(Instant::now());
                    }
                    if let Some(cmd) = tab.agent.as_deref().and_then(session::boot_command) {
                        spawned.write(format!("{cmd}\r"));
                    }
                    self.tabs.push(spawned);
                }
                Err(e) => eprintln!("termi: failed to restore tab: {e}"),
            }
        }
        let alive: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
        self.screens = snap
            .screens
            .into_iter()
            .filter_map(|l| l.retain(&|id| alive.contains(&id)))
            .collect();
        for id in &alive {
            if !self.screens.iter().any(|l| l.contains(*id)) {
                self.screens.push(Layout::Leaf(*id));
            }
        }
        self.active = self
            .tabs
            .iter()
            .position(|t| t.id == snap.active)
            .unwrap_or(0);
        self.next_id = alive.into_iter().max().unwrap_or(0) + 1;
        self.show_tree = snap.tree && !self.tabs.is_empty();
        if self.tabs.is_empty() {
            self.screens.clear();
            self.next_id = 1;
        }
    }

    fn snapshot(&self) -> session::Snapshot {
        session::Snapshot {
            active: self.tabs.get(self.active).map(|t| t.id).unwrap_or(0),
            tree: self.show_tree,
            tabs: self
                .tabs
                .iter()
                .map(|t| session::SavedTab {
                    id: t.id,
                    cwd: t.cwd.clone(),
                    name: t.name.clone(),
                    agent: t
                        .agent
                        .map(|a| a.name().to_owned())
                        .or_else(|| t.held_agent.clone()),
                })
                .collect(),
            screens: self.screens.clone(),
        }
    }

    fn persist_session(&mut self) {
        if self.settings.resume.is_none() || self.tabs.is_empty() {
            return;
        }
        let text = session::serialize(&self.snapshot());
        if text == self.session_saved {
            return;
        }
        session::store(&text);
        self.session_saved = text;
    }

    fn fonts(&self) -> Fonts {
        let size = self.settings.font_size as f32;
        Fonts {
            regular: FontId::new(size, FontFamily::Monospace),
            bold: FontId::new(size, FontFamily::Name("bold".into())),
        }
    }

    fn cell(&self, ctx: &egui::Context) -> Cell {
        ctx.fonts_mut(|f| Cell {
            w: f.glyph_width(&self.fonts().regular, 'M'),
            h: f.row_height(&self.fonts().regular),
        })
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
        let split = if cmd(Key::Pipe)
            || ctx
                .input_mut(|i| i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Backslash))
        {
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
            let next = focus_after_close(self.tabs.len(), self.active);
            self.tabs.remove(self.active);
            if let Some(next) = next {
                self.active = next;
            }
        }
        if cmd(Key::Comma) {
            self.settings_open = if self.settings_open.is_some() {
                None
            } else {
                Some(0)
            };
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
        let nums = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
            Key::Num9,
        ];
        for (i, k) in nums.into_iter().enumerate() {
            if cmd(k) && i < self.tabs.len() {
                self.active = i;
            }
        }
    }
}

/// Split inserts the new tab immediately after the active one. A new screen appends.
fn tab_slot(active: usize, len: usize, split: bool) -> usize {
    if split {
        active.saturating_add(1).min(len)
    } else {
        len
    }
}

/// Column per screen, top to bottom in tab-list order. A pane in no screen is its own trailing column.
fn tab_columns(order: &[u64], screens: &[Layout]) -> Vec<Vec<usize>> {
    let mut used = vec![false; order.len()];
    let mut cols = Vec::new();
    for screen in screens {
        let mut col = Vec::new();
        for (i, id) in order.iter().enumerate() {
            if !used[i] && screen.contains(*id) {
                used[i] = true;
                col.push(i);
            }
        }
        if !col.is_empty() {
            cols.push(col);
        }
    }
    for (i, seen) in used.iter().enumerate() {
        if !seen {
            cols.push(vec![i]);
        }
    }
    cols
}

/// One row is `chip_h + 8`. Each extra row adds `chip_h + 6`.
fn stacked_bar_height(rows: usize, chip_h: f32) -> f32 {
    let rows = rows.max(1) as f32;
    rows * chip_h + (rows - 1.0) * 6.0 + 8.0
}

struct ChipSpot {
    index: usize,
    x: f32,
    y: f32,
    w: f32,
}

/// Left-aligned chips. Column pitch is the widest chip. Offsets are from the bar's top-left.
fn place_columns(columns: &[Vec<usize>], widths: &[f32], chip_h: f32) -> Vec<ChipSpot> {
    let mut spots = Vec::new();
    let mut x = tab_inset(false);
    for col in columns {
        let pitch = col
            .iter()
            .filter_map(|i| widths.get(*i).copied())
            .fold(0.0, f32::max);
        for (row, &index) in col.iter().enumerate() {
            let w = widths.get(index).copied().unwrap_or(0.0);
            let y = 4.0 + row as f32 * (chip_h + 6.0);
            spots.push(ChipSpot { index, x, y, w });
        }
        if !col.is_empty() {
            x += pitch + 6.0;
        }
    }
    spots
}

/// Traffic lights use the one-row band, including when the bar is taller.
fn light_hover(y: f32, chip_h: f32) -> bool {
    y < stacked_bar_height(1, chip_h)
}

/// Directory label on the first row, 16px in from the right, centred in the chip height.
fn cwd_origin(bar: egui::Rect, text: egui::Vec2, chip_h: f32) -> Pos2 {
    Pos2::new(
        bar.right() - 16.0 - text.x,
        bar.top() + 4.0 + (chip_h - text.y) * 0.5,
    )
}

/// Focus stays on the closed slot. The last pane leaves nothing to focus.
fn focus_after_close(len: usize, active: usize) -> Option<usize> {
    (len > 1).then(|| active.min(len - 2))
}

/// One key into the rename draft: Some(true) commit, Some(false) cancel.
fn rename_key(buf: &mut String, ev: &egui::Event) -> Option<bool> {
    match ev {
        egui::Event::Text(t) => buf.push_str(t),
        egui::Event::Paste(t) => buf.push_str(t.lines().next().unwrap_or("")),
        egui::Event::Key {
            key: Key::Backspace,
            pressed: true,
            ..
        } => _ = buf.pop(),
        egui::Event::Key {
            key: Key::Enter,
            pressed: true,
            ..
        } => return Some(true),
        egui::Event::Key {
            key: Key::Escape,
            pressed: true,
            ..
        } => return Some(false),
        _ => {}
    }
    None
}

/// Theme, colours and fonts from the settings. Font size isn't here: it's read every frame.
fn apply(ctx: &egui::Context, s: &settings::Settings) {
    term::set_theme(&s.theme);
    let mut visuals = if term::theme().light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    visuals.panel_fill = if s.glass == "off" {
        background()
    } else {
        Color32::TRANSPARENT
    };
    visuals.window_fill = background();
    visuals.extreme_bg_color = background();
    visuals.override_text_color = Some(foreground());
    ctx.set_visuals(visuals);
    install_fonts(ctx, &s.font);
}

/// `name` from `settings::FONTS`; Meslo backs it up for missing files and glyphs (it has the Nerd Font icons).
fn install_fonts(ctx: &egui::Context, name: &str) {
    install_fonts_at(ctx, name, true);
}

/// `system_tab` loads SF for tab titles when that face is on disk. Snapshot tests pass false:
/// SF rasterisation differs across macOS versions, and the bundled face does not.
fn install_fonts_at(ctx: &egui::Context, name: &str, system_tab: bool) {
    let mut defs = egui::FontDefinitions::default();
    let font = |b: &'static [u8]| Arc::new(egui::FontData::from_static(b));
    defs.font_data.insert(
        "meslo".into(),
        font(include_bytes!("../assets/MesloLGSNerdFontMono-Regular.ttf")),
    );
    defs.font_data.insert(
        "meslo-bold".into(),
        font(include_bytes!("../assets/MesloLGSNerdFontMono-Bold.ttf")),
    );
    let (mut mono, mut bold) = (
        vec!["meslo".to_owned()],
        vec!["meslo-bold".to_owned(), "meslo".to_owned()],
    );
    let file = |path: &str, index: u32| {
        std::fs::read(path).ok().map(|b| {
            Arc::new(egui::FontData {
                index,
                ..egui::FontData::from_owned(b)
            })
        })
    };
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
    // Except tab titles: the system font (SF), at the text optical size.
    let mut tab = vec!["meslo".into()];
    if system_tab && let Ok(b) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
        let tweak = egui::FontTweak {
            coords: egui::epaint::text::VariationCoords::new([("wght", TAB_WGHT), ("opsz", 13.0)]),
            ..Default::default()
        };
        defs.font_data.insert(
            "sf".into(),
            Arc::new(egui::FontData::from_owned(b).tweak(tweak)),
        );
        tab.insert(0, "sf".into());
    }
    defs.families.insert(FontFamily::Name("tab".into()), tab);
    ctx.set_fonts(defs);
}

#[cfg(target_os = "macos")]
fn set_traffic_lights(show: bool) {
    use objc2_app_kit::{NSApplication, NSWindowButton};
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    for w in NSApplication::sharedApplication(mtm).windows().iter() {
        for b in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
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
    let rel = path
        .strip_prefix(&tab.cwd)
        .unwrap_or(path)
        .to_string_lossy();
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
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as usize)
}

impl eframe::App for App {
    /// Unpainted areas are the terminal background, not eframe's default grey.
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        if self.settings.glass == "off" {
            background().to_normalized_gamma_f32()
        } else {
            [0.0; 4]
        }
    }

    fn on_exit(&mut self) {
        self.persist_session();
    }

    #[cfg(target_os = "macos")]
    fn raw_input_hook(&mut self, _: &egui::Context, raw: &mut egui::RawInput) {
        raw.events.extend(menu::take());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let glass = self.settings.glass != "off";
        let time = if self.settings.glass == "wave" {
            ctx.input(|i| i.time)
        } else {
            0.0
        };
        #[cfg(target_os = "macos")]
        glass::blur(
            &mut self.blur,
            _frame,
            glass && self.settings.glass_opacity < 1.0,
        );
        if glass {
            glass::paint(
                ui.painter(),
                ui.max_rect(),
                time,
                term::theme().light,
                self.settings.glass_opacity,
            );
            if self.settings.glass == "wave" && ctx.input(|i| i.viewport().focused.unwrap_or(true))
            {
                ctx.request_repaint_after(Duration::from_millis(33));
            }
        }
        if self.settings.resume.is_none() {
            egui::CentralPanel::no_frame().show(ui, |ui| {
                ui.add_space(4.0);
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                let font = self.fonts().regular;
                if let Some(pick_up) = session::welcome(ui, &font, &mut self.welcome) {
                    self.choose_launch(pick_up, &ctx);
                }
            });
            return;
        }
        #[cfg(target_os = "macos")]
        {
            let paths = menu::take_paths();
            if !paths.is_empty() {
                for path in paths {
                    self.new_tab(&ctx, path, None);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        }
        let panel_fill = if glass {
            Color32::TRANSPARENT
        } else {
            background()
        };
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
        self.screens = std::mem::take(&mut self.screens)
            .into_iter()
            .filter_map(|l| l.retain(&|id| alive.contains(&id)))
            .collect();
        let poll_every = if self.tabs.iter().any(|t| t.agent.is_some()) {
            Duration::from_millis(300)
        } else {
            Duration::from_secs(1)
        };
        if self.last_poll.elapsed() > poll_every {
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

        let cell = self.cell(&ctx);
        let font = self.fonts().regular;
        let tab_font = FontId::new(tab_title_size(font.size), FontFamily::Name("tab".into()));
        let metrics = tab_metrics(&ctx, &tab_font);
        let order: Vec<u64> = self.tabs.iter().map(|t| t.id).collect();
        let columns = tab_columns(&order, &self.screens);
        let rows = columns.iter().map(|c| c.len()).max().unwrap_or(1);
        let bar_h = stacked_bar_height(rows, metrics.chip_h);
        // The light band stays one row when a column stacks below it.
        let show = ctx
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|p| light_hover(p.y, metrics.chip_h));
        if self.lights != Some(show) {
            set_traffic_lights(show);
            self.lights = Some(show);
        }

        let cwd = tilde(&self.tabs[self.active].cwd);
        let chips: Vec<Chip> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let glyph = match (tab.agent.is_some(), tab.state, tab.done) {
                    (true, State::NeedsInput, _) => "! ",
                    (true, State::Idle, true) => "✓ ",
                    _ => "",
                };
                let name = match &self.renaming {
                    Some(buf) if i == self.active => format!("{buf}█"),
                    _ => tab.label(),
                };
                Chip {
                    label: format!("⌘{} {glyph}{name}", i + 1),
                    active: i == self.active,
                    agent: tab.agent,
                    state: tab.state,
                    done: tab.done,
                }
            })
            .collect();
        let bright = term::theme().light;
        let hit = egui::Panel::top("tabs")
            .exact_size(bar_h)
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(egui::Margin {
                        left: 0,
                        right: 0,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| {
                tab_strip(
                    ui, &chips, &columns, &metrics, &tab_font, bright, show, &cwd, &font,
                )
            })
            .inner;
        match hit {
            Some(TabHit::Select(i)) => {
                if self.renaming.is_some() && i != self.active {
                    self.renaming = None;
                }
                self.active = i;
            }
            Some(TabHit::Rename(i)) => {
                self.active = i;
                self.renaming = Some(self.tabs[i].label());
            }
            None => {}
        }

        // ---- tree ----
        if self.show_tree {
            let cwd = self.tabs[self.active].cwd.clone();
            egui::Panel::left("tree")
                .resizable(true)
                .default_size(260.0)
                .frame(
                    egui::Frame::NONE
                        .fill(panel_fill)
                        .inner_margin(egui::Margin {
                            left: 6,
                            right: 6,
                            top: 4,
                            bottom: 0,
                        }),
                )
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
                        if let Some(tree::Action::Open(p)) =
                            self.tree.show(ui, &cwd, &font, &mut self.tree_focus)
                        {
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
                ui.add_space(4.0);
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
                let shown = ed
                    .path
                    .strip_prefix(&tab.cwd)
                    .unwrap_or(&ed.path)
                    .to_string_lossy()
                    .into_owned();
                ui.add_space(4.0);
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
                let Some(i) = self.tabs.iter().position(|t| t.id == id) else {
                    continue;
                };
                self.terminal(ui, &ctx, &cell, i, r, &ids);
            }
            // ⌘ held: the whole screen grabs the mouse; drag a pane onto another's edge to re-split it there.
            // Without ⌘ the panes keep the mouse for text selection.
            let grab_id = egui::Id::new("pane-grab");
            let under = |p: Option<Pos2>| {
                p.and_then(|p| panes.iter().find(|(_, r)| r.contains(p)).copied())
            };
            if panes.len() > 1
                && (ctx.input(|i| i.modifiers.command) || ctx.is_being_dragged(grab_id))
            {
                let resp = ui.interact(area, grab_id, Sense::drag());
                let target = under(ctx.pointer_hover_pos());
                if resp.drag_started() {
                    self.pane_drag =
                        under(ctx.input(|i| i.pointer.press_origin())).map(|(id, _)| id);
                }
                let outline = |r: egui::Rect, c| {
                    ui.painter().rect_stroke(
                        r.shrink(1.0),
                        0.0,
                        egui::Stroke::new(2.0, c),
                        egui::StrokeKind::Inside,
                    )
                };
                match self
                    .pane_drag
                    .filter(|_| resp.dragged() || resp.drag_stopped())
                {
                    Some(src) => {
                        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                        if let Some((_, r)) = panes.iter().find(|(id, _)| *id == src) {
                            outline(*r, Color32::from_gray(90));
                        }
                        if let (Some((dst, r)), Some(p)) = (
                            target.filter(|(dst, _)| *dst != src),
                            ctx.pointer_hover_pos(),
                        ) {
                            // the edge of the target nearest the pointer
                            let d = (p - r.center()) / r.size();
                            let (side, before) = if d.x.abs() > d.y.abs() {
                                (true, d.x < 0.0)
                            } else {
                                (false, d.y < 0.0)
                            };
                            let after = screen.clone().moved(src, dst, side, before);
                            // preview where src lands, in terminal bright blue
                            if let Some((_, land)) = after
                                .rects(area, snap)
                                .into_iter()
                                .find(|(id, _)| *id == src)
                            {
                                ui.painter().rect_filled(
                                    land,
                                    0.0,
                                    Color32::from_rgba_unmultiplied(0x5c, 0x5c, 0xff, 40),
                                );
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
                ui.painter()
                    .line_segment(d, egui::Stroke::new(1.0, Color32::from_gray(70)));
            }
        });
        if central.response.contains_pointer() && ctx.input(|i| i.pointer.any_pressed()) {
            self.tree_focus = false;
        }

        // drag preview for tree → prompt
        if let (Some(path), Some(pos)) = (
            egui::DragAndDrop::payload::<PathBuf>(&ctx),
            ctx.pointer_hover_pos(),
        ) {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("dnd"),
            ));
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            let r = painter.text(
                pos + vec2(12.0, 4.0),
                egui::Align2::LEFT_TOP,
                format!(" {name}"),
                font.clone(),
                background(),
            );
            painter.rect_filled(r.expand(2.0), 0.0, foreground());
            painter.text(
                pos + vec2(12.0, 4.0),
                egui::Align2::LEFT_TOP,
                format!(" {name}"),
                font.clone(),
                background(),
            );
        }

        let busy = self
            .tabs
            .iter()
            .any(|t| t.agent.is_some() && matches!(t.state, State::Working | State::NeedsInput));
        ctx.request_repaint_after(Duration::from_millis(if busy { 33 } else { 1000 }));
        self.persist_session();
    }
}

impl App {
    /// One pane: tab `i` in `rect`. `panes` are the screen's pane ids.
    fn terminal(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        cell: &Cell,
        i: usize,
        rect: egui::Rect,
        panes: &[egui::Id],
    ) {
        let resp = ui.interact(rect, pane_id(self.tabs[i].id), Sense::click_and_drag());
        if resp.clicked() || resp.drag_started() {
            self.active = i;
        }
        let pad = 4.0;
        let origin = rect.min + vec2(4.0, pad);
        let is_active = i == self.active;
        let fonts = self.fonts();
        let tab = &mut self.tabs[i];
        let (cols, rows) = term::grid_size(rect.size() - vec2(8.0, pad), cell);
        tab.resize(cols, rows, cell);

        // focus follows the active pane, unless something else (not a pane) holds it
        if is_active
            && (resp.clicked()
                || ui.memory(|m| {
                    m.focused()
                        .is_none_or(|f| f != resp.id && panes.contains(&f))
                }))
        {
            resp.request_focus();
        }
        let filter = egui::EventFilter {
            tab: true,
            horizontal_arrows: true,
            vertical_arrows: true,
            escape: true,
        };
        ui.memory_mut(|m| m.set_focus_lock_filter(resp.id, filter));

        if resp.has_focus() {
            let app_cursor = tab
                .term
                .lock()
                .mode()
                .contains(alacritty_terminal::term::TermMode::APP_CURSOR);
            for ev in ctx.input(|i| i.events.clone()) {
                match ev {
                    egui::Event::Text(s) => {
                        tab.done = false;
                        tab.paste_raw(&s);
                    }
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if let Some(b) = term::key_bytes(key, modifiers, app_cursor) {
                            tab.done = false;
                            tab.paste_raw(std::str::from_utf8(&b).unwrap_or(""));
                        }
                    }
                    egui::Event::Paste(s) => {
                        tab.done = false;
                        tab.paste(&s);
                    }
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

        if resp.drag_started() {
            let start = ctx.input(|i| i.pointer.press_origin()).unwrap_or(rect.min);
            tab.start_selection(start, origin, cell);
        }
        if resp.dragged() {
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
        term::paint(
            tab,
            &painter,
            origin,
            cell,
            &fonts,
            resp.has_focus() && !self.tree_focus,
        );
        if resp.dnd_hover_payload::<PathBuf>().is_some() {
            painter.rect_stroke(
                rect.shrink(1.0),
                0.0,
                egui::Stroke::new(1.0, foreground()),
                egui::StrokeKind::Inside,
            );
        }
    }
}

fn pane_id(tab: u64) -> egui::Id {
    egui::Id::new(("pane", tab))
}

/// SF variable weight for tab titles. 400 is regular.
const TAB_WGHT: f32 = 400.0;

fn tab_title_size(terminal: f32) -> f32 {
    (terminal - 2.0).max(11.0)
}

fn chip_height(text_h: f32) -> f32 {
    text_h + 6.0
}

/// Claude Code's Clawd. `#` body, `o` eyes, `.` empty. Frame 1 only moves the feet.
fn clawd_rows(frame: usize) -> [&'static str; 5] {
    let feet = if frame % 2 == 0 {
        ".#.#...#.#."
    } else {
        "#.#.....#.#"
    };
    [
        ".#########.",
        "##o#####o##",
        "###########",
        ".#########.",
        feet,
    ]
}

fn mascot_frame(state: State, now_ms: usize) -> usize {
    if state == State::Working {
        (now_ms / 160) % 2
    } else {
        0
    }
}

/// Official Grok mark, already at its tilted angle. `#` is the disc.
const GROK: [&str; 23] = [
    ".......................",
    ".....................#.",
    "....................#..",
    "........#######....##..",
    ".......#########..##...",
    ".....#####.......###...",
    ".....###.........###...",
    "....###.........####...",
    "....##.........#####...",
    "...###........#...##...",
    "...###.......#....##...",
    "...##.......#.....##...",
    "...##......#......##...",
    "...###............##...",
    "....##...........###...",
    "....###..........##....",
    "....###.........###....",
    "....##........####.....",
    "....#..##########......",
    "...#....########.......",
    "..#....................",
    ".......................",
    ".......................",
];

/// One turn of the Grok mark. The phase stays inside a few seconds so it still moves on a Unix clock.
fn saturn_spin(state: State, now_ms: usize) -> f32 {
    if state == State::Working {
        (now_ms % 1600) as f32 / 1600.0 * std::f32::consts::TAU
    } else {
        0.0
    }
}

/// 0 is the terminal background, 1 is the foreground. Working swings between them.
fn contrast_mix(state: State, now_ms: usize) -> f32 {
    if state == State::Working {
        ((now_ms % 900) as f32 / 900.0 * std::f32::consts::TAU).sin() * 0.5 + 0.5
    } else {
        1.0
    }
}

/// Vertical gradient offset. Idle is still. Working scrolls.
fn codex_shift(state: State, now_ms: usize) -> f32 {
    if state == State::Working {
        (now_ms % 1400) as f32 / 1400.0
    } else {
        0.0
    }
}

fn mark_kind(name: &str) -> Option<&'static str> {
    match name {
        "claude" => Some("crab"),
        "codex" => Some("logo"),
        "grok" => Some("saturn"),
        "cursor" => Some("contrast"),
        _ => None,
    }
}

struct TabMetrics {
    chip_h: f32,
}

struct Chip {
    label: String,
    active: bool,
    agent: Option<&'static dyn agent::Agent>,
    state: State,
    done: bool,
}

enum TabHit {
    Select(usize),
    Rename(usize),
}

fn tab_metrics(ctx: &egui::Context, title: &FontId) -> TabMetrics {
    let text_h = ctx.fonts_mut(|f| f.row_height(title));
    TabMetrics {
        chip_h: chip_height(text_h),
    }
}

fn chip_colour(active: bool, state: State, agent: bool, done: bool) -> Color32 {
    match (active, agent, state, done) {
        (_, true, State::Working, _) => pulse(),
        (false, _, State::NeedsInput, _) => Color32::from_rgb(0xcd, 0xcd, 0x00),
        (false, _, State::Idle, true) => Color32::from_rgb(0x00, 0xcd, 0x00),
        (true, _, _, _) => foreground(),
        _ => Color32::from_gray(160),
    }
}

/// Left inset that clears the traffic lights. Constant so the row does not jump on hover.
fn tab_inset(_lights: bool) -> f32 {
    80.0
}

struct DrawnChip {
    colour: Color32,
    galley: Arc<egui::Galley>,
    mascot_w: f32,
    mascot_h: f32,
    w: f32,
}

/// Liquid-glass tab columns. Dragging empty glass moves the window. Returns the tab that was clicked.
fn tab_strip(
    ui: &mut egui::Ui,
    chips: &[Chip],
    columns: &[Vec<usize>],
    metrics: &TabMetrics,
    label_font: &FontId,
    bright: bool,
    _lights: bool,
    cwd: &str,
    cwd_font: &FontId,
) -> Option<TabHit> {
    let bar = ui.max_rect();
    glass::bar(ui.painter(), bar, bright);
    let drag = ui.interact(bar, egui::Id::new("tab-bar"), Sense::drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let drawn: Vec<DrawnChip> = chips
        .iter()
        .map(|chip| {
            let colour = chip_colour(chip.active, chip.state, chip.agent.is_some(), chip.done);
            let galley =
                ui.painter()
                    .layout_no_wrap(chip.label.clone(), label_font.clone(), colour);
            let full = if chip.agent.is_some() {
                (metrics.chip_h - 4.0).max(8.0)
            } else {
                0.0
            };
            let (mascot_w, mascot_h) = if chip.agent.is_some_and(|a| a.name() == "claude") {
                let h = full * 0.72;
                (h * 11.0 / 5.0, h)
            } else {
                (full, full)
            };
            let lead = if chip.agent.is_some() {
                5.0 + mascot_w + 4.0
            } else {
                10.0
            };
            DrawnChip {
                colour,
                w: lead + galley.size().x + 12.0,
                galley,
                mascot_w,
                mascot_h,
            }
        })
        .collect();
    let widths: Vec<f32> = drawn.iter().map(|d| d.w).collect();
    let fallback: Vec<Vec<usize>> = (0..chips.len()).map(|i| vec![i]).collect();
    let columns = if columns.is_empty() {
        &fallback
    } else {
        columns
    };
    let mut hit = None;
    for spot in place_columns(columns, &widths, metrics.chip_h) {
        let (Some(chip), Some(drawn)) = (chips.get(spot.index), drawn.get(spot.index)) else {
            continue;
        };
        let rect =
            egui::Rect::from_min_size(bar.min + vec2(spot.x, spot.y), vec2(spot.w, metrics.chip_h));
        let resp = ui.allocate_rect(rect, Sense::click());
        glass::chip(ui.painter(), rect, bright, chip.active);
        let mut x = rect.left() + if chip.agent.is_some() { 5.0 } else { 10.0 };
        if let Some(a) = chip.agent {
            paint_mascot(
                ui.painter(),
                Pos2::new(x, rect.center().y - drawn.mascot_h * 0.5),
                drawn.mascot_w,
                drawn.mascot_h,
                a,
                chip.state,
                chip.done,
            );
            x += drawn.mascot_w + 4.0;
        }
        ui.painter().galley(
            Pos2::new(x, rect.center().y - drawn.galley.size().y * 0.5),
            drawn.galley.clone(),
            drawn.colour,
        );
        if resp.double_clicked() {
            hit = Some(TabHit::Rename(spot.index));
        } else if resp.clicked() {
            hit = Some(TabHit::Select(spot.index));
        }
    }
    let cwd_colour = Color32::from_gray(120);
    let galley = ui
        .painter()
        .layout_no_wrap(cwd.to_owned(), cwd_font.clone(), cwd_colour);
    ui.painter().galley(
        cwd_origin(bar, galley.size(), metrics.chip_h),
        galley,
        cwd_colour,
    );
    hit
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
    Color32::from_rgb(ch(a.r(), b.r()), ch(a.g(), b.g()), ch(a.b(), b.b()))
}

/// Lavender at 0, deep blue at 1. `shift` scrolls it.
fn codex_gradient(t: f32) -> Color32 {
    let t = t.rem_euclid(1.0);
    let lavender = Color32::from_rgb(196, 181, 253);
    let mid = Color32::from_rgb(99, 102, 241);
    let blue = Color32::from_rgb(37, 99, 235);
    if t < 0.5 {
        mix(lavender, mid, t / 0.5)
    } else {
        mix(mid, blue, (t - 0.5) / 0.5)
    }
}

/// Idle, working, needs-input, done. Needs-input wins; done only shows once the turn has stopped.
fn mascot_mood(state: State, done: bool) -> &'static str {
    match state {
        State::NeedsInput => "input",
        State::Working => "working",
        State::Idle if done => "done",
        State::Idle => "idle",
    }
}

fn accent(mood: &str) -> Option<Color32> {
    match mood {
        "input" => Some(Color32::from_rgb(0xcd, 0xcd, 0x00)),
        "done" => Some(Color32::from_rgb(0x00, 0xcd, 0x00)),
        _ => None,
    }
}

fn paint_mascot(
    painter: &egui::Painter,
    origin: Pos2,
    w: f32,
    h: f32,
    a: &dyn agent::Agent,
    state: State,
    done: bool,
) {
    let now = epoch_ms();
    let mood = mascot_mood(state, done);
    let frame = if mood == "working" {
        mascot_frame(state, now)
    } else {
        0
    };
    let centre = origin + vec2(w * 0.5, h * 0.5);
    match mark_kind(a.name()) {
        Some("crab") => {
            let rows = clawd_rows(frame);
            let pix_w = w / 11.0;
            let pix_h = h / 5.0;
            let y0 = origin.y - if frame == 1 { pix_h * 0.2 } else { 0.0 };
            let rgb = a.color();
            let body = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            let eye = accent(mood).unwrap_or(Color32::from_rgb(28, 18, 16));
            for (y, row) in rows.iter().enumerate() {
                for (x, ch) in row.chars().enumerate() {
                    let colour = match ch {
                        '#' => body,
                        'o' => eye,
                        _ => continue,
                    };
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            Pos2::new(origin.x + x as f32 * pix_w, y0 + y as f32 * pix_h),
                            vec2(pix_w, pix_h),
                        ),
                        0.0,
                        colour,
                    );
                }
            }
        }
        Some("logo") => paint_codex(
            painter,
            centre,
            w.min(h),
            if mood == "working" {
                codex_shift(state, now)
            } else {
                0.0
            },
            accent(mood),
        ),
        Some("saturn") => paint_grok(painter, origin, w, h, saturn_spin(state, now)),
        Some("contrast") => paint_cursor(
            painter,
            centre,
            w.min(h),
            mood,
            if mood == "working" {
                contrast_mix(state, now)
            } else {
                0.0
            },
        ),
        _ => {}
    }
}

/// Cursor's mark: a pointer inside a rounded square. Working swaps the square and the pointer between background and foreground.
fn paint_cursor(painter: &egui::Painter, centre: Pos2, s: f32, mood: &str, mix_t: f32) {
    let side = s * 0.78;
    let (fill, ink) = match mood {
        "working" => (
            mix(background(), foreground(), mix_t),
            mix(foreground(), background(), mix_t),
        ),
        "input" => (
            Color32::from_rgb(48, 40, 0),
            Color32::from_rgb(0xcd, 0xcd, 0x00),
        ),
        "done" => (
            Color32::from_rgb(0, 48, 16),
            Color32::from_rgb(0x00, 0xcd, 0x00),
        ),
        _ => (Color32::from_rgb(24, 24, 28), foreground()),
    };
    let rect = egui::Rect::from_center_size(centre, vec2(side, side));
    painter.rect_filled(rect, side * 0.22, fill);
    painter.rect_stroke(
        rect,
        side * 0.22,
        egui::Stroke::new((s * 0.06).max(1.0), ink),
        egui::StrokeKind::Inside,
    );
    let tip = centre + vec2(-s * 0.06, -s * 0.16);
    let left = centre + vec2(-s * 0.18, s * 0.14);
    let right = centre + vec2(s * 0.10, s * 0.02);
    painter.add(egui::Shape::convex_polygon(
        vec![tip, left, right],
        ink,
        egui::Stroke::NONE,
    ));
}

fn paint_codex(painter: &egui::Painter, centre: Pos2, s: f32, shift: f32, ring: Option<Color32>) {
    let petal = s * 0.34;
    let orbit = s * 0.20;
    let colour_at = |dy: f32| codex_gradient((dy / s + 0.5) + shift);
    for i in 0..6 {
        let a = i as f32 * std::f32::consts::TAU / 6.0;
        let pos = centre + vec2(a.cos(), a.sin()) * orbit;
        painter.circle_filled(pos, petal, colour_at(pos.y - centre.y));
    }
    painter.circle_filled(centre, petal * 0.92, colour_at(0.0));
    let sw = (s * 0.09).max(1.2);
    let white = egui::Stroke::new(sw, Color32::WHITE);
    let a = centre + vec2(-s * 0.18, -s * 0.13);
    let b = centre + vec2(-s * 0.02, s * 0.01);
    let c = centre + vec2(-s * 0.18, s * 0.15);
    painter.line_segment([a, b], white);
    painter.line_segment([b, c], white);
    painter.rect_filled(
        egui::Rect::from_center_size(centre + vec2(s * 0.16, s * 0.04), vec2(s * 0.22, sw)),
        sw * 0.5,
        Color32::WHITE,
    );
    if let Some(c) = ring {
        painter.circle_stroke(centre, s * 0.48, egui::Stroke::new((s * 0.06).max(1.0), c));
    }
}

/// The Grok mark, white. `spin` is extra rotation; idle is the logo's own angle.
fn paint_grok(painter: &egui::Painter, origin: Pos2, w: f32, h: f32, spin: f32) {
    let n = GROK.len() as f32;
    let pix = (w / n).min(h / n);
    let x0 = origin.x + (w - pix * n) * 0.5;
    let y0 = origin.y + (h - pix * n) * 0.5;
    let centre = Pos2::new(x0 + pix * n * 0.5, y0 + pix * n * 0.5);
    let (cs, sn) = (spin.cos(), spin.sin());
    for (y, row) in GROK.iter().enumerate() {
        for (x, ch) in row.chars().enumerate() {
            if ch != '#' {
                continue;
            }
            let dx = (x as f32 + 0.5) * pix - pix * n * 0.5;
            let dy = (y as f32 + 0.5) * pix - pix * n * 0.5;
            let p = centre + vec2(dx * cs - dy * sn, dx * sn + dy * cs);
            painter.rect_filled(
                egui::Rect::from_center_size(p, vec2(pix + 0.4, pix + 0.4)),
                0.0,
                Color32::WHITE,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_lands_beside_the_active_tab() {
        assert_eq!(tab_slot(0, 3, true), 1);
        assert_eq!(tab_slot(1, 3, true), 2);
        assert_eq!(tab_slot(2, 3, true), 3);
        assert_eq!(tab_slot(0, 3, false), 3);
        assert_eq!(tab_slot(0, 0, false), 0);
        let mut ids = vec![1, 2, 4];
        ids.insert(tab_slot(0, ids.len(), true), 3);
        assert_eq!(ids, vec![1, 3, 2, 4]);
    }

    #[test]
    fn split_stacks_under_the_source_and_keeps_the_next_screen() {
        let mut ids = vec![1, 3];
        ids.insert(tab_slot(0, ids.len(), true), 2);
        assert_eq!(ids, vec![1, 2, 3]);
        let screens = [Layout::Leaf(1).split(1, 2, true, false), Layout::Leaf(3)];
        assert_eq!(tab_columns(&ids, &screens), vec![vec![0, 1], vec![2]]);

        ids.insert(tab_slot(0, ids.len(), true), 4);
        assert_eq!(ids, vec![1, 4, 2, 3]);
        let screens = [
            Layout::Leaf(1)
                .split(1, 2, true, false)
                .split(1, 4, false, false),
            Layout::Leaf(3),
        ];
        assert_eq!(tab_columns(&ids, &screens), vec![vec![0, 1, 2], vec![3]]);

        ids.insert(tab_slot(2, ids.len(), true), 5);
        assert_eq!(ids, vec![1, 4, 2, 5, 3]);
        let screens = [
            Layout::Leaf(1)
                .split(1, 2, true, false)
                .split(1, 4, false, false)
                .split(2, 5, true, false),
            Layout::Leaf(3),
        ];
        assert_eq!(tab_columns(&ids, &screens), vec![vec![0, 1, 2, 3], vec![4]]);
    }

    #[test]
    fn drag_keeps_tab_list_order_when_the_tree_changes() {
        let ids = [1, 2, 3];
        let layout = Layout::Leaf(1)
            .split(1, 2, true, false)
            .split(2, 3, false, false);
        let moved = layout.clone().moved(3, 1, true, true);
        let area = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(100.0, 100.0));
        let tree: Vec<u64> = moved
            .rects(area, (1.0, 1.0))
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_ne!(tree, ids);
        assert_eq!(tab_columns(&ids, &[moved]), vec![vec![0, 1, 2]]);
        assert_eq!(tab_columns(&[9], &[]), vec![vec![0]]);
    }

    #[test]
    fn stacked_bar_grows_by_a_chip_and_a_gap() {
        assert_eq!(stacked_bar_height(1, 20.0), 28.0);
        assert_eq!(stacked_bar_height(3, 20.0), 80.0);
        assert!(light_hover(27.0, 20.0));
        assert!(!light_hover(28.0, 20.0));
        assert!(!light_hover(40.0, 20.0));
    }

    #[test]
    fn column_pitch_uses_the_widest_chip_and_chips_stay_narrow() {
        let spots = place_columns(&[vec![0, 1], vec![2]], &[10.0, 30.0, 12.0], 20.0);
        let spot = |i: usize| spots.iter().find(|s| s.index == i).unwrap();
        assert_eq!(spot(0).x, 80.0);
        assert_eq!(spot(0).y, 4.0);
        assert_eq!(spot(0).w, 10.0);
        assert_eq!(spot(1).x, 80.0);
        assert_eq!(spot(1).y, 30.0);
        assert_eq!(spot(1).w, 30.0);
        assert_eq!(spot(2).x, 116.0);
        assert_eq!(spot(2).y, 4.0);
        assert_eq!(spot(2).w, 12.0);
        let bar = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(200.0, 80.0));
        assert_eq!(
            cwd_origin(bar, vec2(40.0, 10.0), 20.0),
            egui::pos2(144.0, 9.0)
        );
    }

    #[test]
    fn close_keeps_the_slot_and_the_last_pane_clears_focus() {
        assert_eq!(focus_after_close(1, 0), None);
        assert_eq!(focus_after_close(4, 0), Some(0));
        assert_eq!(focus_after_close(4, 2), Some(2));
        assert_eq!(focus_after_close(4, 3), Some(2));
    }

    #[test]
    fn tabs_do_not_shift_for_traffic_lights() {
        assert_eq!(tab_inset(false), tab_inset(true));
    }

    #[test]
    fn tab_title_is_regular_and_the_pill_is_one_line() {
        assert_eq!(TAB_WGHT, 400.0);
        assert!(tab_title_size(14.0) < 14.0);
        let h = chip_height(16.0);
        assert!(h > 16.0 && h < 32.0, "{h}");
    }

    #[test]
    fn each_provider_has_its_own_mark() {
        let mut seen = std::collections::HashSet::new();
        for a in agent::AGENTS {
            let kind = mark_kind(a.name()).unwrap_or_else(|| panic!("{}", a.name()));
            assert!(seen.insert(kind), "{kind}");
        }
        assert_eq!(mark_kind("claude"), Some("crab"));
        assert_eq!(mark_kind("grok"), Some("saturn"));
        assert_ne!(mark_kind("grok"), Some("slash"));
        assert!(GROK.iter().any(|row| row.contains('#')));
        assert_eq!(saturn_spin(State::Idle, 0), 0.0);
        assert_eq!(saturn_spin(State::Idle, 0), saturn_spin(State::Idle, 4000));
        assert_ne!(
            saturn_spin(State::Working, 0),
            saturn_spin(State::Working, 1000)
        );
        // the clock is ~1e12 ms; f32 cannot see a few hundred milliseconds there
        assert_ne!(
            saturn_spin(State::Working, 1_763_000_000_000),
            saturn_spin(State::Working, 1_763_000_000_400)
        );
        assert_ne!(
            codex_shift(State::Working, 1_763_000_000_000),
            codex_shift(State::Working, 1_763_000_000_700)
        );
        assert_ne!(
            contrast_mix(State::Working, 1_763_000_000_000),
            contrast_mix(State::Working, 1_763_000_000_262)
        );
        assert_eq!(codex_shift(State::Idle, 2000), 0.0);
        assert!(codex_shift(State::Working, 700) > 0.0);
        assert_eq!(contrast_mix(State::Idle, 900), 1.0);
        assert_ne!(
            contrast_mix(State::Working, 0),
            contrast_mix(State::Working, 262)
        );
        let lum = |name: &str| {
            agent::AGENTS
                .iter()
                .find(|a| a.name() == name)
                .unwrap()
                .color()
                .iter()
                .map(|c| *c as u16)
                .sum::<u16>()
        };
        assert!(lum("grok") > 600);
        assert!(lum("claude") <= 600);
    }

    #[test]
    fn mascot_moods_cover_input_and_done() {
        assert_eq!(mascot_mood(State::Idle, false), "idle");
        assert_eq!(mascot_mood(State::Working, false), "working");
        assert_eq!(mascot_mood(State::NeedsInput, false), "input");
        assert_eq!(mascot_mood(State::Idle, true), "done");
        assert_eq!(mascot_mood(State::NeedsInput, true), "input");
    }

    #[test]
    fn mascots_walk_only_while_working() {
        assert_eq!(mascot_frame(State::Idle, 500), 0);
        assert_eq!(mascot_frame(State::NeedsInput, 500), 0);
        assert_eq!(mascot_frame(State::Working, 0), 0);
        assert_eq!(mascot_frame(State::Working, 160), 1);
        assert_ne!(
            mascot_frame(State::Working, 0),
            mascot_frame(State::Working, 160)
        );
    }

    #[test]
    fn claude_mascot_is_clawd() {
        let idle = clawd_rows(0);
        assert_eq!(idle[0], ".#########.");
        assert_eq!(idle[1], "##o#####o##");
        assert_eq!(idle[4], ".#.#...#.#.");
        let step = clawd_rows(1);
        assert_eq!(&step[..4], &idle[..4]);
        assert_ne!(step[4], idle[4]);
        assert_eq!(step[4].chars().filter(|c| *c == '#').count(), 4);
    }

    #[test]
    fn chip_colour_keeps_state_readable() {
        let _theme = term::THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        term::set_theme("xterm");
        assert_eq!(chip_colour(true, State::Idle, false, false), foreground());
        assert_eq!(
            chip_colour(false, State::NeedsInput, true, false),
            Color32::from_rgb(0xcd, 0xcd, 0x00)
        );
        assert_eq!(
            chip_colour(false, State::Idle, true, true),
            Color32::from_rgb(0x00, 0xcd, 0x00)
        );
        assert_eq!(
            chip_colour(false, State::Idle, false, false),
            Color32::from_gray(160)
        );
        term::set_theme("xterm");
    }

    #[test]
    fn tab_bar_shows_every_provider_mascot() {
        let _theme = term::THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        term::set_theme("xterm");
        let agent = |name: &str| agent::AGENTS.iter().copied().find(|a| a.name() == name);
        let chips = vec![
            Chip {
                label: "⌘1 claude".into(),
                active: true,
                agent: agent("claude"),
                state: State::Idle,
                done: false,
            },
            Chip {
                label: "⌘2 zsh".into(),
                active: false,
                agent: None,
                state: State::Idle,
                done: false,
            },
            Chip {
                label: "⌘3 codex".into(),
                active: false,
                agent: agent("codex"),
                state: State::Idle,
                done: false,
            },
            Chip {
                label: "⌘4 ! grok".into(),
                active: false,
                agent: agent("grok"),
                state: State::NeedsInput,
                done: false,
            },
            Chip {
                label: "⌘5 ✓ cursor".into(),
                active: false,
                agent: agent("cursor"),
                state: State::Idle,
                done: true,
            },
        ];
        // set_fonts applies on the next frame, so the first pass only installs them.
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(980.0, 52.0))
            .build_ui_state(
                move |ui, seen: &mut bool| {
                    apply(ui.ctx(), &settings::Settings::default());
                    // SF differs between this machine and the macOS 15 release runner.
                    install_fonts_at(ui.ctx(), "Meslo", false);
                    if !*seen {
                        *seen = true;
                        return;
                    }
                    ui.painter().rect_filled(ui.max_rect(), 0.0, background());
                    let mono = FontId::monospace(14.0);
                    let label = FontId::new(tab_title_size(14.0), FontFamily::Name("tab".into()));
                    let metrics = tab_metrics(ui.ctx(), &label);
                    let columns: Vec<Vec<usize>> = (0..chips.len()).map(|i| vec![i]).collect();
                    egui::Panel::top("tabs")
                        .exact_size(stacked_bar_height(1, metrics.chip_h))
                        .show_separator_line(false)
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| {
                            tab_strip(
                                ui, &chips, &columns, &metrics, &label, false, false, "~/src",
                                &mono,
                            );
                        });
                },
                false,
            );
        h.run();
        h.snapshot("tab_bar");
        term::set_theme("xterm");
    }

    #[test]
    fn stacked_columns_put_the_split_under_its_screen() {
        let _theme = term::THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        term::set_theme("xterm");
        let chip = |label: &str, active: bool| Chip {
            label: label.into(),
            active,
            agent: None,
            state: State::Idle,
            done: false,
        };
        let chips = vec![
            chip("⌘1 a", true),
            chip("⌘2 bb", false),
            chip("⌘3 c", false),
        ];
        let columns = vec![vec![0, 1], vec![2]];
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(420.0, 90.0))
            .build_ui_state(
                move |ui, seen: &mut bool| {
                    apply(ui.ctx(), &settings::Settings::default());
                    install_fonts_at(ui.ctx(), "Meslo", false);
                    if !*seen {
                        *seen = true;
                        return;
                    }
                    ui.painter().rect_filled(ui.max_rect(), 0.0, background());
                    let mono = FontId::monospace(14.0);
                    let label = FontId::new(tab_title_size(14.0), FontFamily::Name("tab".into()));
                    let metrics = tab_metrics(ui.ctx(), &label);
                    let rows = columns.iter().map(|c| c.len()).max().unwrap_or(1);
                    egui::Panel::top("tabs")
                        .exact_size(stacked_bar_height(rows, metrics.chip_h))
                        .show_separator_line(false)
                        .frame(egui::Frame::NONE)
                        .show(ui, |ui| {
                            tab_strip(
                                ui, &chips, &columns, &metrics, &label, false, false, "~/work",
                                &mono,
                            );
                        });
                },
                false,
            );
        h.run();
        h.snapshot("tab_columns");
        term::set_theme("xterm");
    }

    #[test]
    fn rename_key_edits_commits_cancels() {
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let mut buf = "zsh".to_owned();
        assert_eq!(rename_key(&mut buf, &key(Key::Backspace)), None);
        assert_eq!(rename_key(&mut buf, &egui::Event::Text("!".into())), None);
        assert_eq!(buf, "zs!");
        assert_eq!(rename_key(&mut buf, &key(Key::Enter)), Some(true));
        assert_eq!(rename_key(&mut buf, &key(Key::Escape)), Some(false));
        // a multi-line paste only keeps its first line
        assert_eq!(
            rename_key(&mut buf, &egui::Event::Paste("ab\ncd".into())),
            None
        );
        assert_eq!(buf, "zs!ab");
        let release = egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
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
    fn settings_screen(
        s: settings::Settings,
        cursor: usize,
    ) -> egui_kittest::Harness<'static, settings::Settings> {
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(720.0, 400.0))
            .build_ui_state(
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
    fn welcome_enter_picks_the_highlighted_row() {
        let _theme = term::THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        term::set_theme("xterm");
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(720.0, 220.0))
            .build_ui_state(
                |ui, state: &mut (usize, Option<bool>)| {
                    apply(ui.ctx(), &settings::Settings::default());
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    if let Some(choice) =
                        session::welcome(ui, &FontId::monospace(14.0), &mut state.0)
                    {
                        state.1 = Some(choice);
                    }
                },
                (0, None),
            );
        h.run();
        assert_eq!(*h.state(), (0, None));
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(h.state().1, Some(false));

        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(720.0, 220.0))
            .build_ui_state(
                |ui, state: &mut (usize, Option<bool>)| {
                    apply(ui.ctx(), &settings::Settings::default());
                    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                    if let Some(choice) =
                        session::welcome(ui, &FontId::monospace(14.0), &mut state.0)
                    {
                        state.1 = Some(choice);
                    }
                },
                (0, None),
            );
        h.key_press(Key::ArrowUp);
        h.run();
        assert_eq!(h.state().0, 1);
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(h.state().1, Some(true));
        term::set_theme("xterm");
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
            shot(
                settings::Settings {
                    theme: t.name.into(),
                    ..base.clone()
                },
                3,
                &format!("settings_theme_{}", t.name.replace(' ', "_")),
            );
        }
        for (name, _) in settings::FONTS {
            shot(
                settings::Settings {
                    font: name.into(),
                    ..base.clone()
                },
                2,
                &format!("settings_font_{}", name.replace(' ', "_")),
            );
        }
        shot(
            settings::Settings {
                font_size: 20,
                ..base.clone()
            },
            1,
            "settings_font_size_20",
        );

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
