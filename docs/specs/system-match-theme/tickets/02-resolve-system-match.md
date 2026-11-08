# 02. Resolve which palette system-match shows

Status: ready
Parent: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
Blocked by: [01. Add the light colour palettes](01-light-palettes.md)

## Delivered behaviour

Given the stored theme, the two slots, the match flag, and the OS theme, one resolved palette name is chosen. Empty slots can be filled. No settings row is drawn and the terminal is not repainted.

## What to build

Provide one resolver the settings screen and the running frame both call. Input is the stored `theme`, `theme_dark`, `theme_light`, the match flag, and the OS theme as dark, light, or unknown. Unknown counts as dark.

While matching is off, the resolved palette is the stored `theme`, and an unknown `theme` resolves to `xterm`. The stored string is left as it is.

While matching is on, the resolved palette is the dark slot when the OS theme is dark or unknown, and the light slot when the OS theme is light. A known name on the wrong side resolves to its seed partner from ticket 01. `theme_dark=solarized light` resolves to `solarized dark`. An unknown dark slot resolves to `xterm`. An unknown light slot resolves to `xterm light`.

An empty slot is not an unknown name. It means not seeded. Seeding fills each empty slot from the pair of the resolved `theme` and leaves a non-empty slot unchanged, including a wrong-side or unknown value. `theme=nope` resolves to `xterm` before that seed, so the filled pair is `xterm` and `xterm light`. Seeding reports the values to store. It does not write the settings file.

The resolver does not change `theme` on its own. The caller that turns matching off stores the resolved on-screen name into `theme`. This ticket covers that result: matching off yields the OS-side palette after the rules above, so a dark slot of `nope` yields `xterm` and a dark slot of `solarized light` yields `solarized dark`.

## Acceptance checks

- [ ] Matching off resolves `gruvbox` to `gruvbox` for every OS theme, including unknown.
- [ ] Matching off resolves `nope` to `xterm` and does not replace the stored string.
- [ ] Matching on with OS light resolves to the light slot. OS dark and unknown OS resolve to the dark slot.
- [ ] `theme_dark=solarized light` displays `solarized dark`. `theme_light=gruvbox` displays `gruvbox light`.
- [ ] An unknown dark slot displays `xterm`. An unknown light slot displays `xterm light`.
- [ ] Empty slots seed from the resolved `theme` pair. A non-empty slot is left unchanged on a second seed.
- [ ] `theme=nope` with empty slots seeds `xterm` and `xterm light`.
- [ ] The value written to `theme` when matching turns off is the resolved OS-side name, not the raw slot string.

## Validation

```sh
cargo test
cargo build
```

The new resolver tests pass, and existing `cargo test` stays green. `cargo build` reports zero warnings. Tests that touch the global palette take `THEME_LOCK`.

## Context

- Parent spec: none. Parent ADR: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
- ADRs: [0001. System-match theme preference](../../../adr/0001-system-match-theme.md)
- Relevant code or domain context: `src/term.rs` palette names and seed partners, `src/settings.rs` stored `theme`

## Out of scope

- Drawing the `match system`, `dark`, or `light` rows
- Saving the settings file
- Cursor preview and snapshots
- Calling the resolver from the running frame
