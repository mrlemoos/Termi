# 0001. System-match theme preference

Status: accepted
Date: 2026-11-08
Deciders: Leo

## Context

Termi stores one `theme` string in `~/.config/termi/settings` and paints every surface from one global palette. The colours row cycles `xterm`, `solarized dark`, `solarized light`, and `gruvbox`. Only `solarized light` is light. That flag already selects egui chrome, markdown highlighting, and glass. An unknown name displays `xterm` and stays in the file until the colours row changes it. The ← → keys on a value that is not in the row's list move as if the value were the first entry: → selects the second entry, ← selects the last.

Nothing reads the OS appearance. egui 0.36 already exposes it as `Context::system_theme() -> Option<Theme>`, including live changes from winit's `ThemeChanged` event. The app ignores that value and forces visuals from the named palette.

The settings list is the whole screen while it is open. The sixteen-colour swatch is the only preview of a palette.

## Decision

Add a system-match preference. The stored `theme` remains the palette used while matching is off. While matching is on, the user has two independent slots, `theme_dark` and `theme_light`, and the OS chooses which slot is on screen. An unknown OS theme counts as dark.

### Palettes

Add `xterm light` and `gruvbox light`. Both appear in the colours cycle.

`xterm light` uses background `#ffffff`, foreground `#000000`, and the same sixteen ANSI colours as `xterm`.

`gruvbox light` uses the published alacritty-theme light palette. Background `#fbf1c7`. Foreground `#3c3836`. ANSI colours, in order: `#fbf1c7`, `#cc241d`, `#98971a`, `#d79921`, `#458588`, `#b16286`, `#689d6a`, `#7c6f64`, `#928374`, `#9d0006`, `#79740e`, `#b57614`, `#076678`, `#8f3f71`, `#427b58`, `#3c3836`. ANSI colour 0 equals the background.

The colours cycle order is `xterm`, `xterm light`, `solarized dark`, `solarized light`, `gruvbox`, `gruvbox light`.

These pairs are used only to seed empty slots:

| Either stored name | Dark slot | Light slot |
|---|---|---|
| `xterm`, `xterm light` | `xterm` | `xterm light` |
| `solarized dark`, `solarized light` | `solarized dark` | `solarized light` |
| `gruvbox`, `gruvbox light` | `gruvbox` | `gruvbox light` |

### Settings rows

The row order is notifications, font size, font, `match system`, the theme rows, shell, glass, glass opacity. `match system` uses the same `[x]` and `[ ]` toggle as notifications. Only the stored string `true` turns it on. A missing key stays off.

Matching off shows `colours` and hides `dark` and `light`. Matching on hides `colours` and shows `dark`, then `light`.

`dark` cycles `xterm`, `solarized dark`, `gruvbox`. `light` cycles `xterm light`, `solarized light`, `gruvbox light`. Any mix of the two slots is allowed.

### Storage

The keys are `theme`, `theme_dark`, `theme_light`, and `match_system`. An empty slot means not seeded. The first time matching turns on, each empty slot is filled from the pair of the resolved `theme`, then both slots are saved. `theme=nope` resolves to `xterm` before that seed. A later time matching turns on leaves a non-empty slot as stored.

Matching off writes `theme` to the resolved palette on screen. The cursor is on `match system` for that toggle, so the written name is the OS side, after the resolve rules below. The screen does not jump.

While matching is on, `theme` is ignored until matching turns off and overwrites it.

### Resolve

A known theme on the wrong side displays its partner from the pair table. `theme_dark=solarized light` displays `solarized dark`. An unknown dark value displays `xterm`. An unknown light value displays `xterm light`. The row label keeps the raw string. The swatch shows the resolved palette. The ← → keys then replace the raw string using today's cycle rule.

`theme=nope` with matching off still displays `xterm` and keeps `nope`.

The cursor on `dark` or `light` previews that row's resolved palette, including the settings chrome. Moving off the row, or closing settings, drops the preview. The preview does not write the other slot or `theme`.

One global palette remains what every painter reads. While matching is on, a change to `Context::system_theme()` resolves again and repaints the terminal, chrome, markdown, and glass. `apply` runs for that change while settings is closed. While matching is off, an OS change does not change the palette.

## Alternatives

### One `system` entry in the colours cycle

A fifth name would map the OS onto one fixed dark palette and one fixed light palette. Rejected. The user wanted a separate match flag, then independent dark and light picks. A single entry cannot store gruvbox for dark and solarized light for light.

### Match flag that always swaps to a twin

The named theme would stay, and matching would display that theme's partner when the OS polarity differed. Rejected for the sync case. The settings screen shows two selections while matching is on. The pair table remains only as the seed for empty slots.

### Read the OS theme only at launch

Rejected. winit already delivers `ThemeChanged` and egui already repaints. A launch-only sample leaves `match system` stale after the first OS toggle.

### Unknown OS theme shows the stored palette

Rejected. An unknown OS theme counts as dark and shows the dark slot. Before the first sample this also shows the dark slot.

### Default matching on when the key is missing

Rejected. Existing settings files would change colour on upgrade.

### Readable ANSI black for gruvbox light

The published palette sets ANSI colour 0 to `#fbf1c7`, the same value as the background, so a program that asks for black draws nothing. An alternative keeps colour 0 at `#282828`. Rejected. The published alacritty-theme bytes are the palette.

### Light palettes appended after the current four

Rejected. The → key from `xterm` should land on `xterm light`, the other side of the same family.

### Keep one colours row while matching

The row could show the stored base name, or the resolved name. Rejected. Matching replaces that row with `dark` and `light`.

### Re-seed both slots every time matching turns on

Rejected. Slots persist. Turning matching on again can jump back to those slots after `colours` was changed while matching was off.

### Matching off restores the previous `theme`

Rejected. Matching off writes the palette on screen, so the colours stay still.

### Swatch always shows the OS side

Rejected. The swatch exists to show the colours being picked. The cursor on `dark` or `light` previews that row.

### Wrong-side value displays the first entry, or displays as stored

Rejected. A known name still selects its family, through the partner. An unknown name is the case that displays `xterm` or `xterm light`. Showing the stored palette would let the dark slot display a light palette.

## Consequences

- Old settings files keep their current colours. A missing `match_system` is off, and the new keys can be absent.
- The first → on the colours row from a default install lands on `xterm light`. Tests that expect `solarized dark` change.
- Adding the `match system` row changes every settings snapshot. They are re-recorded with the screen that grows that row.
- gruvbox light ANSI colour 0 is invisible on its background. That is the published palette.
- A non-empty slot survives toggling matching off and on. The on-screen palette can jump when matching turns on again.
- A hand-edited wrong-side name displays the partner and stays in the file until ← → or until matching turns off, which writes the resolved name into `theme`.
- One global palette remains. Windows do not get their own theme.
- No spec. The tickets under `docs/specs/system-match-theme/tickets/` use this ADR as their parent.

## Evidence

- Confirmed design session on 2026-11-08. The choices above are the accepted answers, including the overrides of the recommendations for the published gruvbox bytes and for treating an unknown OS theme as dark.
- [alacritty-theme `gruvbox_light.toml`](https://github.com/alacritty/alacritty-theme/blob/master/themes/gruvbox_light.toml) is the source of the gruvbox light bytes.
- egui 0.36.2 `Context::system_theme` documents the value as the OS theme used when the preference is System. egui-winit 0.36.2 sets it from `WindowEvent::ThemeChanged`.
- Current palettes, the unknown-name fallback, and the settings rows are `src/term.rs` and `src/settings.rs`.

## Linked specs

- None. Tickets: [01](../specs/system-match-theme/tickets/01-light-palettes.md), [02](../specs/system-match-theme/tickets/02-resolve-system-match.md), [03](../specs/system-match-theme/tickets/03-settings-screen.md), [04](../specs/system-match-theme/tickets/04-follow-os-theme.md).
