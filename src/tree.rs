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

#[derive(Default)]
pub struct Tree {
    root: PathBuf,
    open: HashSet<PathBuf>,
    cache: Vec<Row>,
    stamp: Option<Instant>,
}

pub enum Action {
    Open(PathBuf),
}

impl Tree {
    pub fn show(&mut self, ui: &mut egui::Ui, root: &Path, font: &egui::FontId) -> Option<Action> {
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
        ui.label(egui::RichText::new(format!(" {name}")).font(font.clone()).color(crate::term::FG).strong());

        let mut action = None;
        let mut toggle = None;
        for row in &self.cache {
            let file = row.path.file_name().unwrap_or_default().to_string_lossy();
            let icon = match (row.dir, row.open) { (true, true) => "", (true, false) => "", _ => "" };
            let color = if row.dir { egui::Color32::from_rgb(0x5c, 0x5c, 0xff) } else { crate::term::FG };
            let text = egui::RichText::new(format!("{}{icon} {file}", row.prefix)).font(font.clone()).color(color);
            let resp = ui.add(egui::Label::new(text).sense(egui::Sense::click_and_drag()).selectable(false).truncate());
            resp.dnd_set_drag_payload(row.path.clone());
            if resp.clicked() {
                if row.dir { toggle = Some(row.path.clone()) } else { action = Some(Action::Open(row.path.clone())) }
            }
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
        std::fs::remove_dir_all(dir).unwrap();
    }
}
