//! ⌘, settings: a list driven by ↑ ↓ ← → space esc. Stored as `key=value` lines.

use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, FontId, Key, Modifiers, RichText};

use crate::term::{self, background, foreground};

#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    pub notifications: bool,
    pub glass: String,
    pub glass_opacity: f32,
    pub font_size: u8,
    /// A name from `FONTS`.
    pub font: String,
    /// A name from `term::THEMES`.
    pub theme: String,
    /// Program for new tabs; empty = login shell.
    pub shell: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { glass: "off".into(), glass_opacity: 0.86, notifications: true, font_size: 14, font: FONTS[0].0.into(), theme: term::THEMES[0].name.into(), shell: String::new() }
    }
}

pub const FONT_SIZES: std::ops::RangeInclusive<u8> = 8..=32;

/// (name, Some((regular file, face index, bold file, face index))). None = bundled Meslo.
pub const FONTS: [(&str, Option<(&str, u32, &str, u32)>); 4] = [
    ("Meslo", None),
    ("SF Mono", Some((SF_MONO_REGULAR, 0, SF_MONO_BOLD, 0))),
    ("Menlo", Some(("/System/Library/Fonts/Menlo.ttc", 0, "/System/Library/Fonts/Menlo.ttc", 1))),
    ("Monaco", Some(("/System/Library/Fonts/Monaco.ttf", 0, "/System/Library/Fonts/Monaco.ttf", 0))),
];
const SF_MONO_REGULAR: &str = "/System/Applications/Utilities/Terminal.app/Contents/Resources/Fonts/SF-Mono-Regular.otf";
const SF_MONO_BOLD: &str = "/System/Applications/Utilities/Terminal.app/Contents/Resources/Fonts/SF-Mono-Bold.otf";

/// (key, label). Add a field, a row here and an arm in `get`/`set`/`step` to add an option.
const ROWS: [(&str, &str); 7] = [
    ("notifications", "notify when an agent finishes or needs you"),
    ("font_size", "font size"),
    ("font", "font"),
    ("theme", "colours"),
    ("shell", "shell for new tabs"),
    ("glass", "translucent light"),
    ("glass_opacity", "glass opacity (0 clear, 1 solid)"),
];

impl Settings {
    fn get(&self, row: usize) -> String {
        match row {
            0 => self.notifications.to_string(),
            1 => self.font_size.to_string(),
            2 => self.font.clone(),
            3 => self.theme.clone(),
            4 => self.shell.clone(),
            5 => self.glass.clone(),
            _ => self.glass_opacity.to_string(),
        }
    }

    fn set(&mut self, row: usize, v: &str) {
        match row {
            0 => self.notifications = v == "true",
            1 => {
                if let Ok(n) = v.parse::<u8>() {
                    self.font_size = n.clamp(*FONT_SIZES.start(), *FONT_SIZES.end());
                }
            }
            2 => self.font = v.into(),
            3 => self.theme = v.into(),
            4 => self.shell = v.into(),
            5 if ["off", "wave", "still"].contains(&v) => self.glass = v.into(),
            6 => {
                if let Ok(n) = v.parse::<f32>() {
                    if n.is_finite() {
                        self.glass_opacity = n.clamp(0.0, 1.0);
                    }
                }
            }
            _ => {},
        }
    }

    /// What ← → cycle through, for rows that pick from a list.
    fn options(row: usize) -> Vec<String> {
        match row {
            2 => FONTS.iter().filter(|(_, f)| f.is_none_or(|(r, _, _, _)| std::path::Path::new(r).exists())).map(|(n, _)| n.to_string()).collect(),
            3 => term::THEMES.iter().map(|t| t.name.into()).collect(),
            4 => std::iter::once(String::new()).chain(shells(&std::fs::read_to_string("/etc/shells").unwrap_or_default())).collect(),
            5 => ["off", "wave", "still"].map(String::from).to_vec(),
            _ => Vec::new(),
        }
    }

    /// One ← (-1) or → (+1) on `row`.
    fn step(&mut self, row: usize, d: i32) {
        match row {
            0 => self.notifications = !self.notifications,
            1 => self.set(1, &(self.font_size as i32 + d).to_string()),
            6 => self.set(6, &(((self.glass_opacity * 100.0).round() + d as f32) / 100.0).to_string()),
            _ => {
                let opts = Settings::options(row);
                if opts.is_empty() {
                    return;
                }
                let at = opts.iter().position(|o| *o == self.get(row)).unwrap_or(0) as i32;
                self.set(row, &opts[(at + d).rem_euclid(opts.len() as i32) as usize]);
            }
        }
    }

    /// How a row's value reads on screen.
    fn shown(&self, row: usize) -> String {
        match (row, self.get(row).as_str()) {
            (0, "true") => "[x]".into(),
            (0, _) => "[ ]".into(),
            (6, _) => format!("‹ {:.2} ›", self.glass_opacity),
            (4, "") => "‹ login shell ›".into(),
            (_, v) => format!("‹ {v} ›"),
        }
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            if let Some(row) = ROWS.iter().position(|(key, _)| *key == k.trim()) {
                s.set(row, v.trim());
            }
        }
        s
    }

    pub fn serialize(&self) -> String {
        ROWS.iter().enumerate().map(|(i, (k, _))| format!("{k}={}\n", self.get(i))).collect()
    }

    fn path() -> Option<PathBuf> {
        Some(PathBuf::from(std::env::var_os("HOME")?).join(".config/termi/settings"))
    }

    pub fn load() -> Settings {
        Settings::path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(p) = Settings::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&p, self.serialize()) {
            eprintln!("termi: can't save settings: {e}");
        }
    }

    /// Draws the list and handles keys. Returns false when closed.
    pub fn show(&mut self, ui: &mut egui::Ui, font: &FontId, cursor: &mut usize) -> bool {
        let ctx = ui.ctx().clone();
        let key = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        let n = ROWS.len();
        if key(Key::Escape) {
            return false;
        }
        if key(Key::ArrowUp) {
            *cursor = (*cursor + n - 1) % n;
        }
        if key(Key::ArrowDown) {
            *cursor = (*cursor + 1) % n;
        }
        if key(Key::ArrowLeft) {
            self.step(*cursor, -1);
        }
        if key(Key::ArrowRight) || key(Key::Space) {
            self.step(*cursor, 1);
        }
        // a terminal clips long lines, it doesn't wrap them
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        let t = |s: String| RichText::new(s).font(font.clone());
        ui.label(t("── settings ── ↑↓ move · ←→ change · esc close".into()).color(Color32::from_gray(127)));
        ui.label(t(String::new()));
        let w = ROWS.iter().map(|(_, l)| l.chars().count()).max().unwrap_or(0);
        for (i, (_, label)) in ROWS.iter().enumerate() {
            let line = t(format!(" {} {label:<w$}  {} ", if i == *cursor { '>' } else { ' ' }, self.shown(i)));
            ui.label(if i == *cursor { line.color(background()).background_color(foreground()) } else { line.color(foreground()) });
        }
        // the theme's 16 colours, so you can see what you're picking
        ui.label(t(String::new()));
        let mut job = egui::text::LayoutJob::default();
        let fmt = |color| egui::TextFormat { font_id: font.clone(), color, ..Default::default() };
        job.append(&format!("   {:w$}  ", ""), 0.0, fmt(foreground()));
        for c in term::theme().ansi {
            job.append("██", 0.0, fmt(term::hex(c)));
        }
        ui.label(job);
        true
    }
}

/// Program paths in an /etc/shells file.
fn shells(text: &str) -> impl Iterator<Item = String> + '_ {
    text.lines().map(str::trim).filter(|l| l.starts_with('/')).map(String::from)
}

/// Native Notification Center banner.
// ponytail: osascript, so banners are attributed to Script Editor; UNUserNotificationCenter once the .app is signed.
pub fn notify(title: &str, body: &str) {
    let q = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("display notification \"{}\" with title \"{}\"", q(body), q(title));
    let _ = std::process::Command::new("osascript").args(["-e", &script]).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let s = Settings { glass: "wave".into(), glass_opacity: 0.86, notifications: false, font_size: 18, font: "Menlo".into(), theme: "gruvbox".into(), shell: "/bin/bash".into() };
        assert_eq!(s.serialize(), "notifications=false\nfont_size=18\nfont=Menlo\ntheme=gruvbox\nshell=/bin/bash\nglass=wave\nglass_opacity=0.86\n");
        assert_eq!(Settings::parse(&s.serialize()), s);
        assert_eq!(Settings::parse("junk\nunknown=false"), Settings::default());
        // old files: just the notifications line
        assert_eq!(Settings::parse("notifications=false\n"), Settings { notifications: false, ..Settings::default() });
    }

    #[test]
    fn glass_cycles_and_rejects_unknown_values() {
        let mut s = Settings::default();
        s.step(5, 1);
        assert_eq!(s.glass, "wave");
        s.step(5, 1);
        assert_eq!(s.glass, "still");
        s.step(5, 1);
        assert_eq!(s.glass, "off");
        assert_eq!(Settings::parse("glass=invalid").glass, "off");
    }

    #[test]
    fn glass_opacity_is_bounded_and_persisted() {
        for (value, expected) in [("-1", 0.0), ("2", 1.0), ("0.37", 0.37), ("NaN", 0.86), ("inf", 0.86), ("bad", 0.86)] {
            let s = Settings::parse(&format!("glass_opacity={value}"));
            assert_eq!(s.glass_opacity, expected);
            assert_eq!(Settings::parse(&s.serialize()), s);
        }
        let mut s = Settings::parse("glass_opacity=0");
        s.step(6, -1);
        assert_eq!(s.glass_opacity, 0.0);
        s.step(6, 1);
        assert_eq!(s.shown(6), "‹ 0.01 ›");
        s.set(6, "1");
        s.step(6, 1);
        assert_eq!(s.glass_opacity, 1.0);
        s.step(6, -1);
        assert_eq!(s.shown(6), "‹ 0.99 ›");
    }

    #[test]
    fn font_size_is_clamped() {
        assert_eq!(Settings::parse("font_size=200").font_size, 32);
        assert_eq!(Settings::parse("font_size=1").font_size, 8);
        assert_eq!(Settings::parse("font_size=big").font_size, 14);
        let mut s = Settings { font_size: 32, ..Settings::default() };
        s.step(1, 1);
        assert_eq!(s.font_size, 32);
        s.step(1, -1);
        assert_eq!(s.font_size, 31);
    }

    #[test]
    fn step_cycles_both_ways() {
        let mut s = Settings::default();
        s.step(3, 1);
        assert_eq!(s.theme, "solarized dark");
        s.step(3, -1);
        s.step(3, -1);
        assert_eq!(s.theme, "gruvbox");
        s.step(0, 1);
        assert!(!s.notifications);
        // an unknown value restarts from the first option
        s.theme = "nope".into();
        s.step(3, 1);
        assert_eq!(s.theme, "solarized dark");
    }

    #[test]
    fn meslo_is_always_offered() {
        assert_eq!(Settings::options(2)[0], "Meslo");
        assert_eq!(Settings::options(4)[0], "");
    }

    #[test]
    fn etc_shells() {
        let text = "# List of acceptable shells\n\n/bin/bash\n  /bin/zsh \n";
        assert_eq!(shells(text).collect::<Vec<_>>(), ["/bin/bash", "/bin/zsh"]);
    }

    #[test]
    fn shown_values() {
        let s = Settings::default();
        assert_eq!(s.shown(0), "[x]");
        assert_eq!(s.shown(1), "‹ 14 ›");
        assert_eq!(s.shown(4), "‹ login shell ›");
        let s = Settings { notifications: false, shell: "/bin/zsh".into(), ..s };
        assert_eq!(s.shown(0), "[ ]");
        assert_eq!(s.shown(2), "‹ Meslo ›");
        assert_eq!(s.shown(4), "‹ /bin/zsh ›");
    }

    #[test]
    fn parse_trims_and_keeps_values_with_equals() {
        let s = Settings::parse("  theme = gruvbox \nshell=/bin/env a=b\n");
        assert_eq!(s.theme, "gruvbox");
        assert_eq!(s.shell, "/bin/env a=b");
    }

    #[test]
    fn options_only_for_list_rows() {
        assert!(Settings::options(0).is_empty() && Settings::options(1).is_empty());
        assert_eq!(Settings::options(3), term::THEMES.map(|t| t.name.to_string()));
        assert!(Settings::options(4).iter().skip(1).all(|s| s.starts_with('/')));
    }
}
