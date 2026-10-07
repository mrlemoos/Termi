//! Editable file view with per-line review notes that get sent to the agent as a prompt.

use eframe::egui;
use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Color32, FontId, Key, Modifiers, RichText};

use crate::term::foreground;

const DIM: Color32 = Color32::from_rgb(0x7f, 0x7f, 0x7f);
const NOTE: Color32 = Color32::from_rgb(0xcd, 0xcd, 0x00);

pub struct Editor {
    pub path: PathBuf,
    text: String,
    saved: String,
    notes: BTreeMap<usize, String>,
    draft: Option<(usize, String)>,
    cursor_line: usize,
    discard_armed: bool,
    status: String,
}

pub enum Outcome {
    Stay,
    Close,
    /// Close and paste this into the active tab.
    Send(String),
}

/// Review prompt: one bullet per note with the code line for context (lines may shift after edits).
pub fn review(path: &str, text: &str, notes: &BTreeMap<usize, String>) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = format!("Review comments on {path}:\n");
    for (line, note) in notes {
        let code = lines.get(line - 1).map_or("", |l| l.trim());
        out.push_str(&format!("- {path}:{line} `{code}` — {note}\n"));
    }
    out
}

impl Editor {
    pub fn open(path: PathBuf) -> std::io::Result<Editor> {
        let text = std::fs::read_to_string(&path)?;
        Ok(Editor {
            path, saved: text.clone(), text, notes: BTreeMap::new(), draft: None,
            cursor_line: 1, discard_armed: false, status: String::new(),
        })
    }

    fn dirty(&self) -> bool {
        self.text != self.saved
    }

    pub fn show(&mut self, ui: &mut egui::Ui, font: &FontId, shown_path: &str) -> Outcome {
        let ctx = ui.ctx().clone();
        let cmd = |k| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, k));

        if cmd(Key::S) {
            self.status = match std::fs::write(&self.path, &self.text) {
                Ok(()) => { self.saved = self.text.clone(); "saved".into() }
                Err(e) => format!("save failed: {e}"),
            };
        }
        if cmd(Key::Quote) {
            let existing = self.notes.get(&self.cursor_line).cloned().unwrap_or_default();
            self.draft = Some((self.cursor_line, existing));
        }
        if cmd(Key::Enter) && !self.notes.is_empty() {
            return Outcome::Send(review(shown_path, &self.text, &self.notes));
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if self.draft.take().is_none() {
                if !self.dirty() || self.discard_armed {
                    return Outcome::Close;
                }
                self.discard_armed = true;
                self.status = "unsaved changes — ⌘S to save, esc again to discard".into();
            }
        }

        let t = |s: String, c: Color32| RichText::new(s).font(font.clone()).color(c);
        let mark = if self.dirty() { " [+]" } else { "" };
        ui.label(t(format!("── {shown_path}{mark} ── ⌘S save · ⌘' note · ⌘↵ send {} notes · esc close", self.notes.len()), DIM));

        let bottom = if self.draft.is_some() || !self.status.is_empty() { 2.0 } else { 1.0 };
        let row_h = ui.fonts_mut(|f| f.row_height(font));
        let lines = self.text.lines().count().max(1);
        egui::ScrollArea::both().auto_shrink(false).max_height(ui.available_height() - bottom * row_h).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    for n in 1..=lines {
                        let (glyph, color) = if self.notes.contains_key(&n) { ("●", NOTE) } else { (" ", DIM) };
                        let resp = ui.add(egui::Label::new(t(format!("{n:>5}{glyph} "), color)).sense(egui::Sense::click()).selectable(false));
                        if resp.clicked() {
                            self.draft = Some((n, self.notes.get(&n).cloned().unwrap_or_default()));
                        }
                    }
                });
                let lang = self.path.extension().map_or(String::new(), |e| e.to_string_lossy().into_owned());
                let theme = egui_extras::syntax_highlighting::CodeTheme::dark(font.size);
                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                    let mut job = egui_extras::syntax_highlighting::highlight(ui.ctx(), ui.style(), &theme, buf.as_str(), &lang);
                    // egui_extras underlines italic tokens, and there's no italic face anyway
                    job.sections.iter_mut().for_each(|s| (s.format.italics, s.format.underline) = (false, egui::Stroke::NONE));
                    job.wrap.max_width = wrap;
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let out = egui::TextEdit::multiline(&mut self.text)
                    .font(font.clone())
                    .layouter(&mut layouter)
                    .text_color(foreground())
                    .frame(egui::Frame::NONE)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .show(ui);
                if self.draft.is_none() && !out.response.has_focus() {
                    out.response.request_focus();
                }
                if let Some(r) = out.cursor_range {
                    self.cursor_line = 1 + self.text.chars().take(r.primary.index.0).filter(|&c| c == '\n').count();
                }
            });
        });

        if let Some((line, note)) = &mut self.draft {
            let (line, mut commit) = (*line, false);
            ui.horizontal(|ui| {
                ui.label(t(format!("note L{line}> "), NOTE));
                let resp = ui.add(egui::TextEdit::singleline(note).font(font.clone()).text_color(foreground()).frame(egui::Frame::NONE).desired_width(f32::INFINITY));
                resp.request_focus();
                commit = resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            });
            if commit {
                let note = note.trim().to_owned();
                if note.is_empty() { self.notes.remove(&line); } else { self.notes.insert(line, note); }
                self.draft = None;
            }
        } else if !self.status.is_empty() {
            ui.label(t(self.status.clone(), DIM));
        }
        Outcome::Stay
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn review_prompt() {
        let notes = [(2, "rename".to_string())].into_iter().collect();
        assert_eq!(super::review("a.rs", "fn a() {\n    let x = 1;\n}", &notes), "Review comments on a.rs:\n- a.rs:2 `let x = 1;` — rename\n");
    }
}
