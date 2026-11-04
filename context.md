# Context map

Read this map, then relevant domain context only. Follow ancestor AGENTS.md instructions. Do not recursively load every link.

| Scope | Context | Language | Decisions |
|---|---|---|---|
| Repository | This file | [Domain language](docs/Language.md) | [ADRs](docs/adr/README.md) |

Add one row per business domain when needed, pointing to its local context.md. Domain files own purpose, boundaries, contracts, test commands and links to active specs. Technical packages link to owning domain; they do not duplicate its definitions. Shared contracts have one owner.

Small repo: describe purpose, boundaries and validation here. Split only when independent domains need separate loading.

## Active changes

Link relevant [specs](docs/specs/README.md). Completed changes retain durable rationale through ADRs. No automatic deletion.
