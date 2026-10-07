//! `tree`-style file browser. Rows are a pure function of (root, expanded set).

use eframe::egui;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct Row {
    pub prefix: String,
    pub path: PathBuf,
    pub dir: bool,
    pub open: bool,
}

pub fn rows(root: &Path, open: &HashSet<PathBuf>) -> Vec<Row> {
    let mut out = Vec::new();
    walk(root, "", open, &mut out);
    out
}

fn walk(dir: &Path, prefix: &str, open: &HashSet<PathBuf>, out: &mut Vec<Row>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut entries: Vec<(bool, PathBuf)> = rd
        .flatten()
        .filter(|e| !matches!(e.file_name().to_str(), Some(".git" | ".DS_Store")))
        .map(|e| (e.file_type().is_ok_and(|t| t.is_dir()), e.path()))
        .collect();
    entries.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let n = entries.len();
    for (i, (is_dir, path)) in entries.into_iter().enumerate() {
        let last = i + 1 == n;
        let is_open = is_dir && open.contains(&path);
        out.push(Row { prefix: format!("{prefix}{}", if last { "└── " } else { "├── " }), path: path.clone(), dir: is_dir, open: is_open });
        if is_open {
            walk(&path, &format!("{prefix}{}", if last { "    " } else { "│   " }), open, out);
        }
    }
}

#[derive(Clone, Copy)]
pub enum Move {
    Up,
    Down,
    /// Enter: toggle a dir, open a file.
    Toggle,
    /// →: expand a dir, open a file.
    In,
    /// ←: collapse an open dir, else jump to the parent row.
    Out,
}

/// One keypress: new selected row, plus a file to open. Expanding/collapsing edits `open`.
pub fn step(rows: &[Row], open: &mut HashSet<PathBuf>, sel: usize, m: Move) -> (usize, Option<PathBuf>) {
    let Some(row) = rows.get(sel) else { return (0, None) };
    match m {
        Move::Up => (sel.saturating_sub(1), None),
        Move::Down => ((sel + 1).min(rows.len() - 1), None),
        Move::Toggle | Move::In if !row.dir => (sel, Some(row.path.clone())),
        Move::Toggle if row.open => {
            open.remove(&row.path);
            (sel, None)
        }
        Move::Toggle | Move::In => {
            open.insert(row.path.clone());
            (sel, None)
        }
        Move::Out if row.open => {
            open.remove(&row.path);
            (sel, None)
        }
        Move::Out => (rows[..sel].iter().rposition(|r| Some(r.path.as_path()) == row.path.parent()).unwrap_or(sel), None),
    }
}

#[derive(Default)]
pub struct Tree {
    root: PathBuf,
    open: HashSet<PathBuf>,
    cache: Vec<Row>,
    stamp: Option<Instant>,
    /// Selected row, by path so it survives refreshes.
    sel: PathBuf,
}

pub enum Action {
    Open(PathBuf),
}

impl Tree {
    /// `focused`: keys drive the tree. Esc gives focus back, clicking a row takes it.
    pub fn show(&mut self, ui: &mut egui::Ui, root: &Path, font: &egui::FontId, focused: &mut bool) -> Option<Action> {
        // ponytail: re-read the fs every 2s instead of watching it; `notify` crate if that feels laggy.
        if self.root != root || self.stamp.is_none_or(|t| t.elapsed() > Duration::from_secs(2)) {
            if self.root != root {
                self.open.clear();
                self.root = root.to_owned();
            }
            self.cache = rows(&self.root, &self.open);
            self.stamp = Some(Instant::now());
        }
        let name = root.file_name().map_or("/".into(), |n| n.to_string_lossy().into_owned());
        ui.label(egui::RichText::new(format!(" {name}")).font(font.clone()).color(crate::term::foreground()).strong());

        let mut action = None;
        let mut toggle = None;
        let mut sel = self.cache.iter().position(|r| r.path == self.sel).unwrap_or(0);
        let mut moved = false;
        if *focused {
            let keys = [
                (egui::Key::ArrowUp, Move::Up), (egui::Key::K, Move::Up),
                (egui::Key::ArrowDown, Move::Down), (egui::Key::J, Move::Down),
                (egui::Key::Enter, Move::Toggle),
                (egui::Key::ArrowRight, Move::In), (egui::Key::L, Move::In),
                (egui::Key::ArrowLeft, Move::Out), (egui::Key::H, Move::Out),
            ];
            let ctx = ui.ctx().clone();
            for (key, m) in keys {
                while ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, key)) {
                    let (next, file) = step(&self.cache, &mut self.open, sel, m);
                    (sel, moved) = (next, true);
                    action = file.map(Action::Open).or(action);
                    self.stamp = None;
                }
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                *focused = false;
            }
            // modal while focused: only ⌘ shortcuts get past the tree to the terminal/editor
            ctx.input_mut(|i| {
                i.events.retain(|e| match e {
                    egui::Event::Text(_) => false,
                    egui::Event::Key { modifiers, .. } => modifiers.command,
                    _ => true,
                })
            });
        }
        if let Some(r) = self.cache.get(sel) {
            self.sel = r.path.clone();
        }

        for (i, row) in self.cache.iter().enumerate() {
            let file = row.path.file_name().unwrap_or_default().to_string_lossy();
            let icon = match (row.dir, row.open) { (true, true) => "", (true, false) => "", _ => "" };
            let color = if row.dir { egui::Color32::from_rgb(0x5c, 0x5c, 0xff) } else { crate::term::foreground() };
            let mut text = egui::RichText::new(format!("{}{icon} {file}", row.prefix)).font(font.clone()).color(color);
            if i == sel {
                // block cursor when focused, dim bar when not: like a tmux copy-mode selection
                text = if *focused { text.color(crate::term::background()).background_color(crate::term::foreground()) } else { text.background_color(egui::Color32::from_gray(50)) };
            }
            let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click_and_drag()).selectable(false).truncate());
            resp.dnd_set_drag_payload(row.path.clone());
            if i == sel && moved {
                resp.scroll_to_me(None);
            }
            if resp.clicked() {
                self.sel = row.path.clone();
                *focused = true;
                if row.dir { toggle = Some(row.path.clone()) } else { action = Some(Action::Open(row.path.clone())) }
            }
        }
        if action.is_some() {
            *focused = false;
        }
        if let Some(p) = toggle {
            if !self.open.remove(&p) {
                self.open.insert(p);
            }
            self.stamp = None;
        }
        action
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn draws_like_tree() {
        let dir = std::env::temp_dir().join(format!("termi-tree-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("a.txt"), "").unwrap();
        std::fs::write(dir.join("src/main.rs"), "").unwrap();
        let open = [dir.join("src")].into_iter().collect();
        let got: Vec<String> = super::rows(&dir, &open)
            .iter()
            .map(|r| format!("{}{}", r.prefix, r.path.file_name().unwrap().to_string_lossy()))
            .collect();
        assert_eq!(got, ["├── src", "│   └── main.rs", "└── a.txt"]);

        // keys: ← on a child jumps to its dir, ← again collapses, → reopens, → on a file opens it
        use super::{Move, step};
        let mut open = open;
        let rows = super::rows(&dir, &open);
        assert_eq!(step(&rows, &mut open, 1, Move::Out), (0, None));
        assert_eq!(step(&rows, &mut open, 0, Move::Out), (0, None));
        assert!(open.is_empty());
        assert_eq!(step(&rows, &mut open, 0, Move::In), (0, None));
        assert!(open.contains(&dir.join("src")));
        assert_eq!(step(&rows, &mut open, 2, Move::In), (2, Some(dir.join("a.txt"))));
        assert_eq!(step(&rows, &mut open, 2, Move::Down), (2, None));
        assert_eq!(step(&rows, &mut open, 0, Move::Up), (0, None));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
