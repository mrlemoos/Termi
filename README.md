# Termi

macOS terminal for coding agents. No chrome: traffic lights only appear when you hover the top edge.

## Install

```sh
brew tap mrlemoos/termi https://github.com/mrlemoos/Termi
brew install --cask termi
```

## Keys

| Key | Action |
|-----|--------|
| ⌘T / ⌘W | new / close tab |
| ⌘1…⌘9 | jump to tab (the status line always shows them) |
| ⌘B | file tree. Click a file to open it, drag it onto the terminal to insert `@path` |
| ⌘C / ⌘V | copy selection / paste |
| ⌘, | settings (↑↓ move, space toggle, esc close). Saved to `~/.config/termi/settings` |

### Editor

| Key | Action |
|-----|--------|
| ⌘S | save |
| ⌘' or click line number | add note to that line |
| ⌘↵ | paste every note into the agent as a review prompt |
| Esc | close |

## Agents

claude, codex, grok, and cursor-agent are detected from the tab's foreground process. Each one gets its own mascot.

Agent state is read from the screen. Hooks are a fallback: any hook can write `idle`, `working`, or `input` to `$TERMI_STATE_DIR/$TERMI_TAB`.

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

- Dev: `cargo run`
- Release: every push to `main` runs tests and builds an Apple Silicon app, publishes `Termi.zip` to a GitHub production release, and commits its version and SHA-256 to `Casks/termi.rb`. The version uses Cargo's major/minor and adds the workflow run number to its patch. The cask update uses GitHub's token, so it does not trigger another release. Failed runs can be rerun from Actions.
- Local bundle: `sh scripts/bundle.sh` builds `dist/Termi.app` and `dist/Termi.zip`, then updates `Casks/termi.rb`. Set `RELEASE_VERSION` to override the Cargo version.
- Set `SIGNING_IDENTITY` to your certificate name or SHA-1 to sign the local bundle. Without it, bundles use ad hoc signing. Downloads need a Developer ID Application certificate and notarisation to pass Gatekeeper.

## License

The MesloLGS Nerd Font Mono is compiled into the binary (Apache 2.0, see `assets/MESLO-LICENSE.txt`).
