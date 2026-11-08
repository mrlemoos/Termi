# 04. Follow the OS theme while the terminal is open

Status: ready
Parent: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
Blocked by: [01. Add the light colour palettes](01-light-palettes.md), [02. Resolve which palette system-match shows](02-resolve-system-match.md)

## Delivered behaviour

With system match on, the open terminal changes palette when the OS theme changes. With system match off, an OS theme change leaves the palette alone.

## What to build

The running frame uses the ticket 02 resolver and the stored settings. On each frame, read `Context::system_theme()`. When matching is on and the resolved palette differs from the global palette, apply that palette. When matching is off, leave the global palette on the stored `theme`. An unknown OS theme counts as dark and selects the dark slot.

Chrome, markdown highlighting, and glass already follow the global palette's light flag. Updating that palette is the whole of their behaviour for this ticket. There is no full-app harness, and `App` startup starts a shell. The test must not start a shell. Drive the same decision the frame uses with a settings value and an OS theme of dark, light, and unknown, and assert the global palette name. `App::ui` calls that decision. A settings-only harness is not evidence for this ticket.

The frame does not rewrite `theme`, `theme_dark`, or `theme_light` when the OS theme changes. Tests that touch the global palette take `THEME_LOCK`.

## Acceptance checks

- [ ] Matching on, light slot `solarized light`, OS light: the global palette is `solarized light`.
- [ ] The same settings with OS dark or unknown OS: the global palette is the resolved dark slot.
- [ ] Matching off, `theme=gruvbox`, OS light: the global palette stays `gruvbox`.
- [ ] An OS theme change does not modify the stored `theme` or either slot.
- [ ] The assertion runs without starting a shell, and the running frame calls the same decision.

## Validation

```sh
cargo test
cargo build
```

The new frame-decision test passes and existing `cargo test` stays green. `cargo build` reports zero warnings.

## Context

- Parent spec: none. Parent ADR: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
- ADRs: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
- Relevant code or domain context: `src/main.rs` `apply` and `App::ui`; `Context::system_theme`; ticket 02 resolver

## Out of scope

- The settings rows, seeding, and cursor preview
- Snapshot re-records
- Per-window themes
