# Stacked tab columns

Status: complete
Domain: tabs
Owner: Leo
Related ADRs: none
Tickets: none

## Problem

The tab strip is one horizontal row. Every pane is a chip, numbered from its place in the tab list. ⌘\ and ⌘⇧\ insert the new pane immediately after the active pane in that row and add it to the active screen. On screen, ⌘\ places the new pane to the right and ⌘⇧\ places it below. ⌘T appends a new screen. Only the active screen is shown.

Panes opened into the same screen should stack in the tab list under the pane they were split from. They currently sit in the same horizontal row as every other tab.

## Intended behaviour

Each screen is one column, left to right in screen order. Within a column, panes run top to bottom in tab-list order. ⌘\ and ⌘⇧\ insert the new pane directly under the pane that was split. Panes already below that one shift down. ⌘T, the first pane, and a path opened from outside append a column. On screen, ⌘\ stays left|right and ⌘⇧\ stays top|bottom.

The number on a chip is its place in the tab list, starting at 1. The list is column-major, so the chip under ⌘1 is ⌘2 and the next screen's top chip is the next number. ⌘1…⌘9 select that place. ⌘10 and above stay labels only.

The bar grows with the tallest column. One row is the chip height plus 8px, 4px above the chip and 4px below it. Each extra row adds the chip height plus 6px. The terminal starts below the bar. There is no scroller. Columns that pass the right edge continue past it.

A chip is as wide as its label. Chips in a column share a left edge and do not stretch. The next column starts 6px after the widest chip in the previous column. The first column starts 80px from the left of the bar. The gap between stacked chips is 6px.

The directory label stays on the top row, 16px from the right edge of the bar, vertically centred in that row.

Click selects that pane and shows its screen. Double-click and ⌘R still rename that pane. The active chip is the active pane, including one on a lower row.

⌘W closes the active pane only. Chips below move up. Focus stays on that slot: the pane that shifts into it, which is the pane that was below, or the next column's top when the closed pane was the bottom of its column. Closing the top chip leaves the panes that were below it. The column disappears with its last pane. The last pane in the window still quits.

⌘-drag still changes on-screen geometry inside the screen only. The tab list stays in the order the panes were opened.

Traffic lights still appear only while the pointer is in the top row's band, the same height as a one-row bar. A taller bar does not enlarge that band.

Resume shows the saved tab order and the saved screens. The session file format is unchanged. A pane that belongs to no screen is its own column after the screens, so it is still listed.

## Acceptance checks

- [x] Splitting the top pane of a screen inserts the new pane under it, and a later screen stays its own column.
- [x] Splitting a pane that already has a pane below inserts the new one between them.
- [x] Splitting the bottom pane of a column inserts the new pane under it and before the next screen.
- [x] After ⌘-drag changes the layout tree, the column still follows tab-list order.
- [x] A pane in no screen is a column of its own after the screens.
- [x] One row is chip height plus 8px. Three rows are three chip heights plus 12px plus 8px.
- [x] A chip of width 10 under a chip of width 30 stays width 10. The next column starts 6px after the 30-wide chip. The second row starts 6px below the first chip.
- [x] The directory label's left edge is 16px plus its width in from the bar's right edge, on the first row.
- [x] A pointer in the second row does not count as hovering the traffic-light band. The band's height is the one-row bar.
- [x] Closing the only pane returns no focus. Closing a pane focuses the slot it left.
- [x] The stacked tab strip shows two rows for one screen and a second screen on the top row only. The one-row tab-bar snapshot is re-recorded if its pixels change.
- [x] No shell is started to prove the checks above.

## Decisions and contracts

The tab list order is the number order. Each screen occupies one contiguous slice of that list, slices follow screen order, and a slice is top to bottom. Inserting immediately after the source pane preserves the slice. The painter does not renumber chips and does not rewrite a saved list to repair interleaving.

Column membership comes from the screen layouts. Within a screen, order is the relative order of those panes in the tab list. Layout-tree walk order is not list order.

The frame uses the same pure decisions for the strip geometry, the panel height, the traffic-light hover, and ⌘W focus. Chip rectangles use each chip's own width. Column pitch uses the widest chip in that column. The left inset stays 80px whether or not the traffic lights are visible.

The glass bar fills the stacked height. Empty space under a shorter column is just bar. The directory label does not move down and does not gain collision avoidance with the chips.

Both split shortcuts share the list insertion. `side_by_side` on the layout is unchanged.

No ADR. The choice is tab-list presentation. It does not change the session format, the layout tree, or a cross-cutting invariant beyond the list order already produced by insertion.

## Test seam

Pure functions in `src/main.rs`, beside `tab_slot`, are the seam. They take tab ids, screen layouts, chip widths, chip height, and the closed index. They return columns, chip rectangles, bar height, the traffic-light band, directory position, and focus after close. `App` paints and closes through those results. This matches the existing `tab_slot` and `tab_strip` tests: no shell, and `tab_strip` already has an egui snapshot harness.

`tab_slot` remains the insertion rule. Column tests build a list with it, then group that list by `Layout`. The drag check builds a layout with `Layout::moved` and asserts the column still follows the untouched id list, including a case where the tree walk order differs.

The snapshot harness calls `tab_strip` directly. One new snapshot is a two-row column beside a one-pane column. `tests/snapshots/tab_bar.png` stays the mascot row and is updated only if the one-row pixels change.

Red: the new assertions fail while the functions return the old flat row. Green: `cargo test` passes, then `cargo build` reports zero warnings.

## Validation

```sh
cargo test
cargo build
```

`cargo test` passed, 72 tests, including the column, close, bar-height, and `tab_strip` snapshot tests. `cargo build` completed with zero warnings. `tests/snapshots/tab_columns.png` shows ⌘2 under ⌘1 and ⌘3 on the top row. `tests/snapshots/tab_bar.png` was re-recorded for the directory label.

## Exclusions

- A scroller in either direction.
- Stretching a chip to its column width.
- Indenting a split pane.
- A different stack for ⌘\ than for ⌘⇧\.
- Changing on-screen pane geometry, including ⌘-drag.
- Reordering the tab list when a pane is dragged.
- Closing the rest of a column when its top chip closes.
- A shortcut past ⌘9.
- A session file change, or a resume pass that sorts tabs.
- A README change.
- Moving the directory label onto a lower row.

## Unresolved decisions

None.
