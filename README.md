# Termi

macOS terminal for coding agents. No chrome: traffic lights only appear when you hover the top edge.

| Key | Action |
|-----|--------|
| ⌘T / ⌘W | new / close tab |
| ⌘1…⌘9 | jump to tab (the status line always shows them) |
| ⌘B | file tree. Click a file to open it, drag it onto the terminal to insert `@path` |
| ⌘C / ⌘V | copy selection / paste |
| ⌘, | settings (↑↓ move, space toggle, esc close). Saved to `~/.config/termi/settings` |

**Editor:** ⌘S saves. ⌘' (or clicking a line number) adds a note to that line. ⌘↵ pastes every note into the agent as a review prompt. Esc closes.

**Agents:** claude, codex, grok, and cursor-agent are detected from the tab's foreground process. Each one gets its own mascot. Agent state is read from the screen. Hooks are a fallback: any hook can write `idle`, `working`, or `input` to `$TERMI_STATE_DIR/$TERMI_TAB`.

Claude Code example (`~/.claude/settings.json`):

```json
{
  "hooks": {
    "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "[ -n \"$TERMI_TAB\" ] && echo working > \"$TERMI_STATE_DIR/$TERMI_TAB\" || true" }] }],
    "Notification":     [{ "hooks": [{ "type": "command", "command": "[ -n \"$TERMI_TAB\" ] && echo input > \"$TERMI_STATE_DIR/$TERMI_TAB\" || true" }] }],
    "Stop":             [{ "hooks": [{ "type": "command", "command": "[ -n \"$TERMI_TAB\" ] && echo idle > \"$TERMI_STATE_DIR/$TERMI_TAB\" || true" }] }]
  }
}
```

## Build

`cargo run` for dev. `scripts/bundle.sh` builds `dist/Termi.app` and `dist/Termi.zip`, then stamps the version and sha256 into `Casks/termi.rb`. Upload the zip to GitHub release `v<version>`.

Install: `brew tap mrlemoos/termi https://github.com/mrlemoos/Termi && brew install --cask termi`

The MesloLGS Nerd Font Mono is compiled into the binary (Apache 2.0, see `assets/MESLO-LICENSE.txt`).
