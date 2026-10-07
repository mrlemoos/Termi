//! ⌘, settings: a checkbox list driven by ↑ ↓ space esc. Stored as `key=true|false` lines.

use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, FontId, Key, Modifiers, RichText};

use crate::term::{BG, FG};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Settings {
    pub notifications: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { notifications: true }
    }
}

impl Settings {
    /// (key, label, value). Add a field + a row here to add an option.
    fn rows(&mut self) -> [(&'static str, &'static str, &mut bool); 1] {
        [("notifications", "macOS notifications when an agent finishes or needs you", &mut self.notifications)]
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            for (key, _, val) in s.rows() {
                if key == k.trim() {
                    *val = v.trim() == "true";
                }
            }
        }
        s
    }

    pub fn serialize(mut self) -> String {
        self.rows().iter().map(|(k, _, v)| format!("{k}={v}\n")).collect()
    }

    fn path() -> Option<PathBuf> {
        Some(PathBuf::from(std::env::var_os("HOME")?).join(".config/termi/settings"))
    }

    pub fn load() -> Settings {
        Settings::path().and_then(|p| std::fs::read_to_string(p).ok()).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save(self) {
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
        let n = self.rows().len();
        if key(Key::Escape) {
            return false;
        }
        if key(Key::ArrowUp) {
            *cursor = (*cursor + n - 1) % n;
        }
        if key(Key::ArrowDown) {
            *cursor = (*cursor + 1) % n;
        }
        if key(Key::Space) {
            let v = &mut self.rows()[*cursor].2;
            **v = !**v;
            self.save();
        }
        let t = |s: String| RichText::new(s).font(font.clone());
        ui.label(t("── settings ── ↑↓ move · space toggle · esc close".into()).color(Color32::from_gray(127)));
        ui.label(t(String::new()));
        for (i, (_, label, val)) in self.rows().into_iter().enumerate() {
            let line = t(format!(" {} [{}] {label} ", if i == *cursor { '>' } else { ' ' }, if *val { 'x' } else { ' ' }));
            ui.label(if i == *cursor { line.color(BG).background_color(FG) } else { line.color(FG) });
        }
        true
    }
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
    use super::Settings;

    #[test]
    fn roundtrip() {
        let off = Settings { notifications: false };
        assert_eq!(off.serialize(), "notifications=false\n");
        assert_eq!(Settings::parse(&off.serialize()), off);
        assert_eq!(Settings::parse("junk\nunknown=false"), Settings::default());
    }
}
