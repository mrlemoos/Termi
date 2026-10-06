//! Agent facade: one trait, one impl per CLI. Everything else is plain functions.

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Idle,
    Working,
    NeedsInput,
}

pub trait Agent: Sync {
    fn name(&self) -> &'static str;
    fn color(&self) -> [u8; 3];
    /// Does the tab's foreground command line belong to this agent?
    fn matches(&self, cmd: &str) -> bool;
    /// Screen text that means "busy" / "waiting on you".
    // ponytail: string heuristics, tune when an agent changes its TUI copy.
    fn busy_marks(&self) -> &'static [&'static str];
    fn ask_marks(&self) -> &'static [&'static str];
    /// Two animation frames; frame 1 only shows while working.
    fn sprite(&self, frame: usize) -> [&'static str; 3];

    fn busy(&self, screen: &str) -> bool {
        self.busy_marks().iter().any(|m| screen.contains(m))
    }

    fn read_screen(&self, screen: &str) -> Option<State> {
        if self.ask_marks().iter().any(|m| screen.contains(m)) {
            Some(State::NeedsInput)
        } else if self.busy(screen) {
            Some(State::Working)
        } else {
            None
        }
    }

    /// How a dragged file is typed into this agent's prompt.
    fn file_ref(&self, path: &str) -> String {
        format!("@{path} ")
    }
}

/// argv[0] basename starts with `bin`, or argv[0] is a JS runtime whose script path mentions `bin`.
fn runs(cmd: &str, bin: &str) -> bool {
    let mut argv = cmd.split_whitespace();
    let exe = argv.next().unwrap_or("").rsplit('/').next().unwrap_or("");
    match exe {
        "node" | "bun" | "deno" => argv.next().is_some_and(|script| script.contains(bin)),
        _ => exe.starts_with(bin),
    }
}

struct Claude;
struct Codex;
struct Grok;
struct Cursor;

impl Agent for Claude {
    fn name(&self) -> &'static str { "claude" }
    fn color(&self) -> [u8; 3] { [0xd9, 0x77, 0x57] }
    fn matches(&self, cmd: &str) -> bool { runs(cmd, "claude") }
    fn busy_marks(&self) -> &'static [&'static str] { &["esc to interrupt"] }
    fn ask_marks(&self) -> &'static [&'static str] { &["Do you want to", "❯ 1. Yes"] }
    /// Status line while working: `✳ Deliberating…`, `✻ Jitterbugging… (12s · ↓ 4 tokens)`.
    /// When done it turns into `✻ Baked for 3s · done`, which has no `…` word.
    fn busy(&self, screen: &str) -> bool {
        screen.lines().any(|l| {
            let mut words = l.split_whitespace();
            words.next().is_some_and(|g| matches!(g, "✳" | "✻" | "✢" | "✶" | "✽" | "·" | "*"))
                && words.next().is_some_and(|w| w.ends_with('…'))
        })
    }
    fn sprite(&self, f: usize) -> [&'static str; 3] {
        [[" ▐▛███▜▌ ", "▝▜█████▛▘", "  ▘▘ ▝▝  "], [" ▐▛███▜▌ ", "▝▜█████▛▘", " ▝▘  ▘▝  "]][f % 2]
    }
}

impl Agent for Codex {
    fn name(&self) -> &'static str { "codex" }
    fn color(&self) -> [u8; 3] { [0xe8, 0xe8, 0xe8] }
    fn matches(&self, cmd: &str) -> bool { runs(cmd, "codex") }
    fn busy_marks(&self) -> &'static [&'static str] { &["esc to interrupt"] }
    fn ask_marks(&self) -> &'static [&'static str] { &["Allow command", "Would you like to run", "approve"] }
    fn sprite(&self, f: usize) -> [&'static str; 3] {
        [["╭──────╮", "│ >_   │", "╰──────╯"], ["╭──────╮", "│ >    │", "╰──────╯"]][f % 2]
    }
}

impl Agent for Grok {
    fn name(&self) -> &'static str { "grok" }
    fn color(&self) -> [u8; 3] { [0xff, 0xff, 0xff] }
    fn matches(&self, cmd: &str) -> bool { runs(cmd, "grok") }
    fn busy_marks(&self) -> &'static [&'static str] { &["esc to interrupt", "Thinking"] }
    fn ask_marks(&self) -> &'static [&'static str] { &["Allow", "(y/n)"] }
    fn sprite(&self, f: usize) -> [&'static str; 3] {
        [[" ╭──╱╮ ", " │ ╱ │ ", " ╰╱──╯ "], [" ╭╲──╮ ", " │ ╲ │ ", " ╰──╲╯ "]][f % 2]
    }
    // grok takes plain paths, no @-mentions
    fn file_ref(&self, path: &str) -> String {
        shell_quote(path)
    }
}

impl Agent for Cursor {
    fn name(&self) -> &'static str { "cursor" }
    fn color(&self) -> [u8; 3] { [0x9a, 0xa4, 0xb4] }
    // cursor-agent often runs as `node …/cursor-agent/…/index.js`
    fn matches(&self, cmd: &str) -> bool { cmd.contains("cursor-agent") }
    fn busy_marks(&self) -> &'static [&'static str] { &["ctrl+c to stop", "Generating"] }
    fn ask_marks(&self) -> &'static [&'static str] { &["Run this command?", "(y)"] }
    fn sprite(&self, f: usize) -> [&'static str; 3] {
        [[" ▗▟█▙▖ ", " ▐█▀█▌ ", " ▝▜█▛▘ "], [" ▗▟█▙▖ ", " ▐█▄█▌ ", " ▝▜█▛▘ "]][f % 2]
    }
}

/// Path + trailing space, single-quoted only when the shell would mangle it.
pub fn shell_quote(path: &str) -> String {
    if path.contains(|c: char| c.is_whitespace() || "'\"$`\\".contains(c)) {
        format!("'{}' ", path.replace('\'', r"'\''"))
    } else {
        format!("{path} ")
    }
}

pub static AGENTS: [&dyn Agent; 4] = [&Claude, &Codex, &Grok, &Cursor];

pub fn detect(cmd: &str) -> Option<&'static dyn Agent> {
    AGENTS.iter().copied().find(|a| a.matches(cmd))
}

/// Hook fallback: any agent hook may write idle|working|input to $TERMI_STATE_DIR/$TERMI_TAB.
pub fn state_dir() -> std::path::PathBuf {
    std::env::temp_dir().join("termi")
}

pub fn hook_state(tab: u64) -> Option<State> {
    match std::fs::read_to_string(state_dir().join(tab.to_string())).ok()?.trim() {
        "working" => Some(State::Working),
        "input" => Some(State::NeedsInput),
        "idle" => Some(State::Idle),
        _ => None,
    }
}

/// Screen first, hook as backup, idle otherwise.
pub fn resolve(agent: &dyn Agent, screen: &str, tab: u64) -> State {
    agent.read_screen(screen).or_else(|| hook_state(tab)).unwrap_or(State::Idle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_reads() {
        assert_eq!(detect("/opt/homebrew/bin/claude --resume").unwrap().name(), "claude");
        assert_eq!(detect("node /usr/lib/node_modules/@openai/codex/bin/codex.js").unwrap().name(), "codex");
        assert_eq!(detect("node /Users/x/.local/share/cursor-agent/versions/1/index.js").unwrap().name(), "cursor");
        assert_eq!(detect("/Users/x/.grok/bin/grok").unwrap().name(), "grok");
        assert!(detect("-zsh").is_none());
        assert!(detect("vim claude.md").is_none());
        assert_eq!(detect("node /usr/lib/node_modules/@anthropic-ai/claude-code/cli.js").unwrap().name(), "claude");
        let c = detect("claude").unwrap();
        assert_eq!(c.read_screen("✻ Thinking… (esc to interrupt)"), Some(State::Working));
        assert_eq!(c.read_screen("⏺ ok\n✳ Deliberating…\n❯"), Some(State::Working));
        assert_eq!(c.read_screen("✻ Jitterbugging… (running Stop hooks… 1/2 · 1s)"), Some(State::Working));
        assert_eq!(c.read_screen("⏺ The command printed hi.\n✻ Baked for 3s · done 1:52 AM\n❯"), None);
        assert_eq!(c.read_screen("Do you want to proceed?\n❯ 1. Yes"), Some(State::NeedsInput));
        assert_eq!(c.read_screen("> "), None);
        assert_eq!(c.file_ref("src/a.rs"), "@src/a.rs ");
        assert_eq!(detect("grok").unwrap().file_ref("my dir/a.rs"), "'my dir/a.rs' ");
    }
}
