# Alluvium-managed vault · schema for AI

> This file is **inside the user's vault** — Alluvium generates it on `init`.
> It is NOT the Alluvium project's CLAUDE.md (that one lives in the Alluvium
> repo root and governs how AI builds Alluvium itself).
>
> Purpose of this file: when an AI agent (Claude Code, Claude Desktop, MCP-
> connected Claude, etc.) reads this vault, it should follow the conventions
> below.

## Layout

- `raw/sessions/` — immutable transcript archives. **Never edit.** Treat as
  evidence/citations only.
- `wiki/concepts/` — ideas, patterns, techniques.
- `wiki/entities/` — projects, tools, libraries, people, organizations.
- `wiki/sources/` — optional per-session summary pages (only if user opts in).
- `wiki/index.md` — auto-generated catalog. Do not edit by hand.
- `wiki/log.md` — append-only timeline. Do not edit historical entries.
- `wiki/overview.md` — high-level summary. Editable; will be revised on `consolidate`.

## Frontmatter schema

Every wiki page has the following YAML frontmatter:

```yaml
title: <human title>
type: concept | entity | source | meta
tags: [<3-7 thematic tags>]
created: <ISO date>
updated: <ISO date>
sources:
  - "[[../raw/sessions/<id>]]"
relations:
  uses: ["[[..]]"]
  used-by: ["[[..]]"]
  related: ["[[..]]"]
  supersedes: ["[[..]]"]
```

- Wikilinks in `relations` carry semantics. Don't conflate with raw `[[X]]`
  references in body text.

## Operating on this vault

- **To find topic info**: start at `wiki/index.md`, follow links to
  `wiki/concepts/` or `wiki/entities/`.
- **For "what happened recently"**: read `wiki/log.md` (newest at bottom).
- **For grounding facts**: read concept and entity pages.
- **For original transcripts**: `raw/sessions/<date>_<id>.md`.

## Conventions

- Page filenames are `kebab-case.md` matching the slug used in the body's
  `# Title`.
- Tags are also `kebab-case`.
- Dates are ISO 8601 (`YYYY-MM-DD` or `YYYY-MM-DDTHH:MM:SSZ`).
- All wikilinks use Obsidian shortlink form: `[[page-slug]]`. Long-form
  `[[../wiki/concepts/page-slug.md|Display]]` only when disambiguation needed.
