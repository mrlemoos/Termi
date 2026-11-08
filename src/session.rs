//! Last close: tabs, splits, and a foreground agent session. The first launch asks
//! which of those to reopen.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, FontId, Key, Modifiers, RichText};

use crate::split::Layout;
use crate::term::{background, foreground};

pub const CHOICES: [&str; 2] = ["clean slate", "pick up where you left off"];

#[derive(Clone, Debug, PartialEq)]
pub struct SavedTab {
    pub id: u64,
    pub cwd: PathBuf,
    pub name: Option<String>,
    pub agent: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub active: u64,
    pub tree: bool,
    pub tabs: Vec<SavedTab>,
    pub screens: Vec<Layout>,
}

/// Continues that agent's latest session in the tab's directory. Anything else reopens the shell only.
pub fn boot_command(agent: &str) -> Option<&'static str> {
    match agent {
        "claude" => Some("claude -c"),
        "codex" => Some("codex resume --last"),
        "grok" => Some("grok -c"),
        "cursor" => Some("cursor-agent --continue"),
        _ => None,
    }
}

/// Draws the first-launch choice. `Some(true)` picks up, `Some(false)` is a clean slate.
pub fn welcome(ui: &mut egui::Ui, font: &FontId, cursor: &mut usize) -> Option<bool> {
    let ctx = ui.ctx().clone();
    let key = |k| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
    let n = CHOICES.len();
    if key(Key::ArrowUp) {
        *cursor = (*cursor + n - 1) % n;
    }
    if key(Key::ArrowDown) {
        *cursor = (*cursor + 1) % n;
    }
    let choose = key(Key::Enter) || key(Key::Space);
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
    let t = |s: String| RichText::new(s).font(font.clone());
    ui.label(t("── termi ──".into()).color(Color32::from_gray(127)));
    ui.label(t(String::new()));
    ui.label(t("  when termi opens".into()).color(foreground()));
    ui.label(t(String::new()));
    for (i, label) in CHOICES.iter().enumerate() {
        let line = t(format!(" {} {label}", if i == *cursor { '>' } else { ' ' }));
        ui.label(if i == *cursor {
            line.color(background()).background_color(foreground())
        } else {
            line.color(foreground())
        });
    }
    ui.label(t(String::new()));
    ui.label(t("  ↑↓ move · enter choose".into()).color(Color32::from_gray(127)));
    choose.then_some(*cursor == 1)
}

pub fn serialize(s: &Snapshot) -> String {
    let mut out = format!("active={}\ntree={}\n", s.active, s.tree);
    for t in &s.tabs {
        out.push_str(&format!("tab={} cwd={}\n", t.id, t.cwd.display()));
        if let Some(name) = &t.name {
            if !name.is_empty() && !name.chars().any(|c| c == '\n' || c == '\r') {
                out.push_str(&format!("tab={} name={name}\n", t.id));
            }
        }
        if let Some(agent) = &t.agent {
            if !agent.is_empty() && !agent.chars().any(|c| c == '\n' || c == '\r') {
                out.push_str(&format!("tab={} agent={agent}\n", t.id));
            }
        }
    }
    for screen in &s.screens {
        out.push_str(&format!("screen={}\n", encode_layout(screen)));
    }
    out
}

pub fn parse(text: &str) -> Option<Snapshot> {
    let mut active = 0;
    let mut tree = false;
    let mut tabs: Vec<SavedTab> = Vec::new();
    let mut screens = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (k, v) = line.split_once('=')?;
        match k.trim() {
            "active" => active = v.trim().parse().ok()?,
            "tree" => tree = v.trim() == "true",
            "screen" => screens.push(parse_layout(v.trim())?),
            "tab" => {
                let (id, rest) = v.trim().split_once(' ')?;
                let id = id.parse().ok()?;
                let (field, value) = rest.split_once('=')?;
                let tab = touch(&mut tabs, id);
                match field {
                    "cwd" => tab.cwd = PathBuf::from(value),
                    "name" if !value.is_empty() => tab.name = Some(value.to_owned()),
                    "agent" if !value.is_empty() => tab.agent = Some(value.to_owned()),
                    "name" | "agent" => {}
                    _ => return None,
                }
            }
            _ => {}
        }
    }
    Some(Snapshot {
        active,
        tree,
        tabs,
        screens,
    })
}

/// Drops tabs whose directory is gone, and screens that only held those tabs.
pub fn openable(mut s: Snapshot, is_dir: impl Fn(&Path) -> bool) -> Option<Snapshot> {
    s.tabs
        .retain(|t| !t.cwd.as_os_str().is_empty() && is_dir(&t.cwd));
    if s.tabs.is_empty() {
        return None;
    }
    let alive: Vec<u64> = s.tabs.iter().map(|t| t.id).collect();
    s.screens = s
        .screens
        .into_iter()
        .filter_map(|l| l.retain(&|id| alive.contains(&id)))
        .collect();
    for id in &alive {
        if !s.screens.iter().any(|l| l.contains(*id)) {
            s.screens.push(Layout::Leaf(*id));
        }
    }
    if !alive.contains(&s.active) {
        s.active = alive[0];
    }
    Some(s)
}

fn path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("HOME")?).join(".config/termi/session"))
}

pub fn load() -> Option<Snapshot> {
    let path = path()?;
    let Ok(text) = std::fs::read_to_string(&path) else {
        return None;
    };
    match parse(&text) {
        Some(s) => Some(s),
        None => {
            eprintln!("termi: ignored unreadable session file {}", path.display());
            None
        }
    }
}

pub fn store(text: &str) {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    match std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, &path)) {
        Ok(()) => {}
        Err(e) => eprintln!("termi: can't save session: {e}"),
    }
}

fn touch(tabs: &mut Vec<SavedTab>, id: u64) -> &mut SavedTab {
    if let Some(i) = tabs.iter().position(|t| t.id == id) {
        return &mut tabs[i];
    }
    tabs.push(SavedTab {
        id,
        cwd: PathBuf::new(),
        name: None,
        agent: None,
    });
    tabs.last_mut().unwrap()
}

fn encode_layout(l: &Layout) -> String {
    match l {
        Layout::Leaf(id) => id.to_string(),
        Layout::Split { side_by_side, a, b } => {
            format!(
                "({} {} {})",
                if *side_by_side { "h" } else { "v" },
                encode_layout(a),
                encode_layout(b)
            )
        }
    }
}

fn parse_layout(s: &str) -> Option<Layout> {
    let mut p = Scan { s, i: 0 };
    let layout = p.layout()?;
    p.skip();
    (p.i == p.s.len()).then_some(layout)
}

struct Scan<'a> {
    s: &'a str,
    i: usize,
}

impl Scan<'_> {
    fn rest(&self) -> &str {
        &self.s[self.i..]
    }

    fn skip(&mut self) {
        self.i += self.rest().len() - self.rest().trim_start().len();
    }

    fn bump(&mut self, c: char) -> bool {
        self.skip();
        if self.rest().starts_with(c) {
            self.i += c.len_utf8();
            true
        } else {
            false
        }
    }

    fn layout(&mut self) -> Option<Layout> {
        self.skip();
        if self.bump('(') {
            self.skip();
            let side = if self.rest().starts_with('h') {
                self.i += 1;
                true
            } else if self.rest().starts_with('v') {
                self.i += 1;
                false
            } else {
                return None;
            };
            let a = Box::new(self.layout()?);
            let b = Box::new(self.layout()?);
            self.bump(')').then(|| Layout::Split {
                side_by_side: side,
                a,
                b,
            })
        } else {
            let start = self.i;
            while self
                .rest()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
            {
                self.i += 1;
            }
            if self.i == start {
                return None;
            }
            Some(Layout::Leaf(self.s[start..self.i].parse().ok()?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_keeps_splits_names_and_agents() {
        let snap = Snapshot {
            active: 2,
            tree: true,
            tabs: vec![
                SavedTab {
                    id: 1,
                    cwd: PathBuf::from("/Users/leo/My Projects"),
                    name: None,
                    agent: None,
                },
                SavedTab {
                    id: 2,
                    cwd: PathBuf::from("/tmp"),
                    name: Some("api".into()),
                    agent: Some("claude".into()),
                },
            ],
            screens: vec![Layout::Leaf(1).split(1, 2, true, false)],
        };
        let again = parse(&serialize(&snap)).unwrap();
        assert_eq!(again, snap);
        assert_eq!(encode_layout(&snap.screens[0]), "(h 1 2)");
    }

    #[test]
    fn nested_layout_and_unknown_keys() {
        let text = "active=3\nfuture=yes\ntree=false\ntab=1 cwd=/keep\ntab=3 cwd=/also\ntab=3 name=a=b\nscreen=(h 1 (v 3 9))\n";
        let snap = parse(text).unwrap();
        assert_eq!(encode_layout(&snap.screens[0]), "(h 1 (v 3 9))");
        assert_eq!(snap.tabs[1].name.as_deref(), Some("a=b"));
        assert!(!snap.tree);
    }

    #[test]
    fn openable_drops_missing_directories_and_repairs_active() {
        let snap =
            parse("active=2\ntab=1 cwd=/keep\ntab=2 cwd=/gone\ntab=2 agent=grok\nscreen=(h 1 2)\n")
                .unwrap();
        let open = openable(snap, |p| p.ends_with("keep")).unwrap();
        assert_eq!(open.tabs.len(), 1);
        assert_eq!(open.active, 1);
        assert_eq!(open.screens, vec![Layout::Leaf(1)]);
        let gone = parse("tab=1 cwd=/gone\n").unwrap();
        assert!(openable(gone, |_| false).is_none());
    }

    #[test]
    fn malformed_session_is_rejected() {
        assert!(parse("tab=nope\n").is_none());
        assert!(parse("screen=(\n").is_none());
        assert!(parse("active=no\n").is_none());
        assert!(parse("").unwrap().tabs.is_empty());
    }

    #[test]
    fn boot_commands_are_the_continue_flags() {
        assert_eq!(boot_command("claude"), Some("claude -c"));
        assert_eq!(boot_command("codex"), Some("codex resume --last"));
        assert_eq!(boot_command("grok"), Some("grok -c"));
        assert_eq!(boot_command("cursor"), Some("cursor-agent --continue"));
        assert_eq!(boot_command("zsh"), None);
        assert_eq!(boot_command("claude; rm -rf /"), None);
    }
}
