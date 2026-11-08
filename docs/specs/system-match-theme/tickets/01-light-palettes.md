# 01. Add the light colour palettes

Status: ready
Parent: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
Blocked by: none

## Delivered behaviour

The colours cycle offers six palettes. A default install moves from `xterm` to `xterm light` on the first →. An unknown name still displays `xterm`.

## What to build

Add `xterm light` and `gruvbox light` to the global palette list. Both are light palettes. The colours cycle order is `xterm`, `xterm light`, `solarized dark`, `solarized light`, `gruvbox`, `gruvbox light`.

`xterm light` uses background `#ffffff`, foreground `#000000`, and the same sixteen ANSI colours as `xterm`: `#000000`, `#cd0000`, `#00cd00`, `#cdcd00`, `#0000ee`, `#cd00cd`, `#00cdcd`, `#e5e5e5`, `#7f7f7f`, `#ff0000`, `#00ff00`, `#ffff00`, `#5c5cff`, `#ff00ff`, `#00ffff`, `#ffffff`.

`gruvbox light` uses the published alacritty-theme light palette recorded in the parent ADR. Background `#fbf1c7`. Foreground `#3c3836`. ANSI colours, in order: `#fbf1c7`, `#cc241d`, `#98971a`, `#d79921`, `#458588`, `#b16286`, `#689d6a`, `#7c6f64`, `#928374`, `#9d0006`, `#79740e`, `#b57614`, `#076678`, `#8f3f71`, `#427b58`, `#3c3836`. ANSI colour 0 equals the background.

Each of the six palettes records the seed partner from the parent ADR. The partners are `xterm` with `xterm light`, `solarized dark` with `solarized light`, and `gruvbox` with `gruvbox light`. This ticket does not seed settings. It only makes the partner available to the resolver.

`set_theme("nope")` still displays `xterm` and leaves the stored string unchanged. Tests that call `set_theme` take `THEME_LOCK`. The existing → expectation that `xterm` leads to `solarized dark` becomes `xterm light`. The settings snapshot pass records one image per palette, including the two new names, and re-records the → shot.

## Acceptance checks

- [ ] The colours options are exactly `xterm`, `xterm light`, `solarized dark`, `solarized light`, `gruvbox`, `gruvbox light`.
- [ ] `xterm light` background is `#ffffff`, foreground is `#000000`, and its ANSI colours match `xterm`.
- [ ] `gruvbox light` background is `#fbf1c7`, foreground is `#3c3836`, ANSI colour 0 is `#fbf1c7`, and the other ANSI colours match the parent ADR.
- [ ] `xterm light` and `gruvbox light` are light. The four existing palettes keep their current light flag.
- [ ] The seed partner of each palette is the other name in its ADR pair.
- [ ] `set_theme("nope")` displays `xterm`.
- [ ] One → from the default colours value selects `xterm light`. One ← from `xterm` selects `gruvbox light`.
- [ ] Settings snapshots exist for all six palette names, and the → snapshot shows `xterm light`.

## Validation

```sh
cargo test
cargo build
```

`cargo test` is green, including the palette assertions and the settings snapshot pass. `cargo build` reports zero warnings. New or changed snapshot images are part of the evidence.

## Context

- Parent spec: none. Parent ADR: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
- ADRs: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
- Relevant code or domain context: `src/term.rs` palette table and `set_theme`, `src/settings.rs` colours options, `src/main.rs` `settings_snapshots`

## Out of scope

- The `match system`, `dark`, and `light` rows
- Seeding, resolve rules, and reading the OS theme
- Re-recording snapshots that change only because a later ticket adds a row
