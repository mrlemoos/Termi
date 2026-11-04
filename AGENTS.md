# AGENTS.md

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`

- Types: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `build`, `ci`, `chore`.
- Scope is optional and is usually the module: `term`, `agent`, `tree`, `editor`, `ui`.
- Summary: imperative mood, lowercase, no trailing period, at most 72 characters.
- Breaking change: add `!` after the type/scope and a `BREAKING CHANGE:` footer.
- Commit and push straight to `main`. No feature branches or PRs unless asked.

## Code

- Prefer functional style: pure functions, data in and data out. Use traits for polymorphism only where it's real (the `Agent` facade).
- Everything on screen must look like a terminal: monospace, grid-aligned, no widgets that look like GUI chrome.
- Run `cargo test` and `cargo build` with zero warnings before committing.


<!-- galicia:start -->
## Galicia

Read context.md first; load relevant domain context only. Domain terms: docs/Language.md. Decisions: docs/adr/. Specs/tickets: docs/specs/. Start new records from .galicia/templates/adr.md, spec.md, or ticket.md. Local Markdown only. No emojis. Preserve checks, exact contracts and uncertainty. Before consequential ADR work, recommend scoped small-model research/review; spawn only when authorised.

Startup review fallback: run `node .galicia/check.ts` once per session if no startup hook ran. Metadata flags are review candidates; never delete without explicit user approval. Follow relevant specs to compare implementation when needed; age alone proves nothing. Grilling cadence: read .galicia/config.json, ask user if unset.
<!-- galicia:end -->
