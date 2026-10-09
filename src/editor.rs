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
    preview: bool,
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
        let preview = markdown_path(&path);
        Ok(Editor {
            path, saved: text.clone(), text, notes: BTreeMap::new(), draft: None,
            cursor_line: 1, discard_armed: false, status: String::new(),
            preview,
        })
    }

    fn dirty(&self) -> bool {
        self.text != self.saved
    }

    pub fn navigate(&mut self, path: PathBuf) {
        if self.dirty() {
            self.status = "unsaved changes — ⌘S to save, or esc twice to discard before opening another file".into();
            return;
        }
        match Self::open(path) {
            Ok(editor) => *self = editor,
            Err(e) => self.status = format!("open failed: {e}"),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, font: &FontId, shown_path: &str) -> Outcome {
        let ctx = ui.ctx().clone();
        let cmd = |k| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, k));
        let markdown = markdown_path(&self.path);
        if markdown && cmd(Key::E) { self.preview = !self.preview; }

        if cmd(Key::S) {
            self.status = match std::fs::write(&self.path, &self.text) {
                Ok(()) => { self.saved = self.text.clone(); "saved".into() }
                Err(e) => format!("save failed: {e}"),
            };
        }
        if !self.preview && cmd(Key::Quote) {
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
        let note_hint = if self.preview { "" } else { " · ⌘' note" };
        ui.label(t(format!("── {shown_path}{mark} ── ⌘S save{note_hint} · ⌘↵ send {} notes · esc close", self.notes.len()), DIM));
        if markdown {
            ui.horizontal(|ui| {
                for (label, preview) in [("preview", true), ("source", false)] {
                    let color = if self.preview == preview { foreground() } else { DIM };
                    if ui.add(egui::Label::new(t(label.into(), color)).sense(egui::Sense::click()).selectable(false)).clicked() {
                        self.preview = preview;
                    }
                    if preview { ui.label(t("|".into(), DIM)); }
                }
                ui.label(t("⌘E toggle · source: ⌘' or line number to note".into(), DIM));
            });
        }

        let bottom = if self.draft.is_some() || !self.status.is_empty() { 2.0 } else { 1.0 };
        let row_h = ui.fonts_mut(|f| f.row_height(font));
        let lines = self.text.lines().count().max(1);
        let mut destination = None;
        egui::ScrollArea::both().id_salt((&self.path, self.preview)).auto_shrink(false).max_height(ui.available_height() - bottom * row_h).show(ui, |ui| {
            if self.preview {
                destination = crate::markdown::show(ui, &self.text, font);
                return;
            }
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
                    .id_salt(&self.path)
                    .font(font.clone())
                    .layouter(&mut layouter)
                    .text_color(foreground())
                    .frame(egui::Frame::NONE)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .show(ui);
                if out.response.changed() { self.discard_armed = false; }
                if self.draft.is_none() && !out.response.has_focus() {
                    out.response.request_focus();
                }
                if let Some(r) = out.cursor_range {
                    self.cursor_line = 1 + self.text.chars().take(r.primary.index.0).filter(|&c| c == '\n').count();
                }
            });
        });

        if let Some(link) = destination {
            if link.starts_with("https://") || link.starts_with("http://") || link.starts_with("mailto:") {
                ctx.open_url(egui::OpenUrl::new_tab(link));
            } else {
                match local_markdown(&self.path, &link) {
                    Some(path) => self.navigate(path),
                    None => self.status = "link unavailable — use a relative Markdown path or http(s) URL".into(),
                }
            }
        }

        if let Some((line, note)) = &mut self.draft.as_mut().filter(|_| !self.preview) {
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

fn markdown_path(path: &std::path::Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
}

fn local_markdown(current: &std::path::Path, link: &str) -> Option<PathBuf> {
    let path = link.split(['#', '?']).next()?;
    if path.is_empty() || path.contains(':') || path.starts_with('/') { return None; }
    let mut bytes = Vec::new();
    let mut chars = path.bytes();
    while let Some(c) = chars.next() {
        bytes.push(if c == b'%' {
            let hex = [chars.next()?, chars.next()?];
            u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?
        } else { c });
    }
    let path = PathBuf::from(String::from_utf8(bytes).ok()?);
    if path.is_absolute() || !markdown_path(&path) { return None; }
    Some(current.parent()?.join(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_links_decode_paths_and_reject_unsupported_destinations() {
        let current = std::path::Path::new("/tmp/docs/readme.md");
        assert_eq!(local_markdown(current, "../other%20file.MD#section"), Some(PathBuf::from("/tmp/docs/../other file.MD")));
        for link in ["https://example.com/a.md", "javascript:bad.md", "/a.md", "//host/a.md", "%2Fa.md", "#section", "a.rs", "%xx.md", "%ff.md"] {
            assert_eq!(local_markdown(current, link), None, "{link}");
        }
    }

    #[test]
    fn navigation_preserves_unsaved_edits_and_reports_errors() {
        let dir = std::env::temp_dir().join(format!("termi-navigation-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first.md");
        let next = dir.join("next.markdown");
        std::fs::write(&first, "# First").unwrap();
        std::fs::write(&next, "# Next").unwrap();
        let mut editor = Editor::open(first.clone()).unwrap();
        assert!(editor.preview);
        editor.text.push_str(" edited");
        editor.notes.insert(1, "note".into());
        editor.navigate(next.clone());
        assert_eq!(editor.path, first);
        assert_eq!(editor.text, "# First edited");
        assert_eq!(editor.notes[&1], "note");
        assert!(editor.status.starts_with("unsaved changes"));
        editor.saved = editor.text.clone();
        editor.navigate(dir.join("missing.md"));
        assert!(editor.status.starts_with("open failed:"));
        assert_eq!(editor.path, first);
        editor.navigate(next.clone());
        assert_eq!(editor.path, next);
        assert_eq!(editor.text, "# Next");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn preview_source_toggle_preserves_buffer_notes_and_saves_source() {
        let _theme = crate::term::THEME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let path = std::env::temp_dir().join(format!("termi-preview-{}.md", std::process::id()));
        std::fs::write(&path, "# Saved").unwrap();
        let mut editor = Editor::open(path.clone()).unwrap();
        editor.text = "# Unsaved **Markdown**".into();
        editor.notes.insert(1, "keep".into());
        editor.preview = false;
        let mut h = egui_kittest::Harness::builder().build_ui_state(|ui, editor: &mut Editor| {
            editor.show(ui, &FontId::monospace(14.0), "preview.md");
        }, editor);
        crate::apply(&h.ctx, &crate::settings::Settings::default());
        h.state_mut().preview = true;
        h.run();
        h.render().unwrap().save("/tmp/termi-markdown-preview.png").unwrap();
        h.key_press_modifiers(Modifiers::COMMAND, Key::E);
        h.run();
        assert!(!h.state().preview);
        h.render().unwrap().save("/tmp/termi-markdown-source.png").unwrap();
        h.key_press_modifiers(Modifiers::COMMAND, Key::E);
        h.run();
        assert!(h.state().preview);
        assert_eq!(h.state().text, "# Unsaved **Markdown**");
        assert_eq!(h.state().notes[&1], "keep");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Saved");
        h.key_press_modifiers(Modifiers::COMMAND, Key::S);
        h.run();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Unsaved **Markdown**");
        assert!(!h.state().dirty());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn review_prompt() {
        let notes = [(2, "rename".to_string())].into_iter().collect();
        assert_eq!(super::review("a.rs", "fn a() {\n    let x = 1;\n}", &notes), "Review comments on a.rs:\n- a.rs:2 `let x = 1;` — rename\n");
    }

    #[test]
    fn review_orders_notes_and_survives_shifted_lines() {
        // a note past the end (lines deleted after it was written) keeps its line number, empty code
        let notes = [(9, "gone".to_string()), (1, "first".to_string())].into_iter().collect();
        assert_eq!(super::review("b", "x", &notes), "Review comments on b:\n- b:1 `x` — first\n- b:9 `` — gone\n");
    }

    #[test]
    fn open_reads_file_clean() {
        let p = std::env::temp_dir().join(format!("termi-editor-{}", std::process::id()));
        std::fs::write(&p, "hi\n").unwrap();
        let ed = super::Editor::open(p.clone()).unwrap();
        assert_eq!(ed.text, "hi\n");
        assert!(!ed.dirty());
        std::fs::remove_file(&p).unwrap();
        assert!(super::Editor::open(p).is_err());
    }
}
