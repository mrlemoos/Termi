# AGENTS.md

## Commits

Use [Conventional Commits](https://www.conventionalcommits.org/): `type(scope): summary`

- Types: `feat`, `fix`, `refactor`, `perf`, `docs`, `test`, `build`, `ci`, `chore`.
- Scope is optional and is usually the module: `term`, `agent`, `tree`, `editor`, `ui`.
- Summary: imperative mood, lowercase, no trailing period, at most 72 characters.
- Breaking change: add `!` after the type/scope and a `BREAKING CHANGE:` footer.

## Code

- Prefer functional style: pure functions, data in and data out. Use traits for polymorphism only where it's real (the `Agent` facade).
- Everything on screen must look like a terminal: monospace, grid-aligned, no widgets that look like GUI chrome.
- Run `cargo test` and `cargo build` with zero warnings before committing.
