# Domain language

Define business concepts, not frameworks, libraries or implementation types. One authoritative definition per term. Root glossary links domain-owned glossaries when needed.

| Term | Meaning | Example | Distinction |
|---|---|---|---|
| screen | One split layout of panes. The window shows the screen that contains the active pane. | ⌘T opens a screen. | A screen is not an OS window. |
| pane | One terminal. It belongs to exactly one screen. | ⌘\ adds a pane to the active screen. | A pane is not a screen. |
| column | The tab-list stack for one screen, top to bottom. | A pane opened with ⌘\ sits under the pane that was split. | A column is not the left-right arrangement of panes on screen. |
| tab | The list entry for one pane. Its number is its place in the tab list. | The chip labelled ⌘2. | A tab is not a screen. Several tabs can share one screen. |

Record ambiguity as unresolved. Agree meaning before updating contracts or acceptance checks. Use these terms consistently in code, specs, tickets and ADRs.
