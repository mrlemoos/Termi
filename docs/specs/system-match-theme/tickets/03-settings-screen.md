# 03. Settings screen for system match

Status: ready
Parent: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
Blocked by: [01. Add the light colour palettes](01-light-palettes.md), [02. Resolve which palette system-match shows](02-resolve-system-match.md)

## Delivered behaviour

The settings screen can turn system match on, pick the dark and light palettes independently, and show the palette that will be on screen. Closing the screen leaves the stored slots intact.

## What to build

Add `match_system`, `theme_dark`, and `theme_light` to the settings file. A missing `match_system` is off. Only the string `true` turns it on. A missing or empty slot means not seeded. After a seed, both slot values remain in the file while matching is off, and a reload restores them.

The row order is notifications, font size, font, `match system`, the theme rows, shell, glass, glass opacity. `match system` uses the same `[x]` and `[ ]` toggle as notifications, including ←, →, and space. Matching off shows `colours` and hides `dark` and `light`. Matching on hides `colours` and shows `dark`, then `light`.

`dark` cycles `xterm`, `solarized dark`, `gruvbox`. `light` cycles `xterm light`, `solarized light`, `gruvbox light`. The ← → keys on a value that is not in that row's list keep today's rule: the move starts as if the value were the first entry. The row label shows the raw stored string. The swatch shows the resolved palette from ticket 02.

The first time matching turns on, each empty slot is seeded from ticket 02 and the file is saved. A later toggle leaves a non-empty slot unchanged. Turning matching off writes `theme` to the resolved palette on screen. The cursor is on `match system` for that toggle, so the written name is the OS side.

The cursor on `dark` or `light` previews that row's resolved palette, including the settings chrome. The preview updates the global palette when the cursor moves, even when the stored settings did not change. Moving to any other row, or closing settings, drops the preview and shows the OS side. The preview does not write the other slot or `theme`. An unknown OS theme counts as dark.

While settings is open and the cursor is not previewing a slot, a change of OS theme updates the swatch and chrome to the newly resolved palette. The OS theme is `Context::system_theme()`. The settings harness can set it through `Harness::input_mut().system_theme` before the next frame. egui copies that field into `Context::system_theme` at the start of the frame.

Every settings snapshot is re-recorded because the list grows a row. The harness is tall enough that every visible row and the swatch are on screen. Include a matching-on shot with both slot rows, and a shot whose swatch is the preview of the slot under the cursor.

Parsing an old file that only sets `theme=gruvbox` yields that theme, matching off, and empty slots.

## Acceptance checks

- [ ] An old `theme=gruvbox` file loads as gruvbox, matching off, with empty slots.
- [ ] `match_system=true` is on. A missing key, `false`, and any other string are off.
- [ ] Matching off shows `colours` and hides `dark` and `light`. Matching on shows `dark`, then `light`, and hides `colours`. `match system` stays directly under font.
- [ ] The dark and light cycles contain only the names in the parent ADR, in that order.
- [ ] The first enable from `theme=gruvbox` saves `theme_dark=gruvbox` and `theme_light=gruvbox light`. A second enable does not replace a non-empty slot.
- [ ] Turning matching off stores the resolved OS-side name in `theme`. With an unknown OS theme, that name is the resolved dark slot.
- [ ] A wrong-side slot still displays its partner, and the row label keeps the raw string until ← or →.
- [ ] The cursor on `light` while the OS theme is dark previews the resolved light palette. Moving to shell, or closing settings, shows the dark slot. The stored slots are unchanged.
- [ ] Seeded slots round-trip while matching is off.
- [ ] The re-recorded settings snapshots show the new row, and the list is not clipped.

## Validation

```sh
cargo test
cargo build
```

Settings parse, cycle, and harness tests pass. The snapshot pass has been re-recorded and is green. `cargo build` reports zero warnings. Tests that touch the global palette take `THEME_LOCK`.

## Context

- Parent spec: none. Parent ADR: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
- ADRs: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
- Relevant code or domain context: `src/settings.rs` rows, parse, and serialize; `src/main.rs` `settings_screen` and `apply`; ticket 02 resolver

## Out of scope

- Changing palette bytes or seed partners
- Repainting the terminal after settings has closed because the OS theme changed
- Per-window themes
