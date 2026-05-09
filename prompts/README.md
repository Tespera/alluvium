# Prompts

This directory holds all LLM prompts that Alluvium uses, **as data files**
(not as Rust string literals). The intent is twofold:

1. **You can edit prompts without recompiling.** The Alluvium binary loads
   these at runtime via `minijinja`.
2. **You can read what your AI is being told.** No magic strings buried in code.

## Layout

- `distill.toml` — main extraction prompt (`src/distiller/prompt.rs` loads it).
- `merge.toml` — merge a new fact into an existing topic page.
- `classify.toml` — entity vs. concept classification.
- `recipes/<name>.toml` — preset variants. Pick one in your config:
  ```toml
  # ~/.config/alluvium/config.toml
  [default]
  recipe = "dev-journal"
  ```

Recipes set `inherits = "default"` and override only specific fields.

## Conventions

- **`[meta]`** — name, description, model.
- **`[byte_caps]`** — pre-prompt truncation limits per field. Tuning these
  affects cost and context usage.
- **`[output_schema]`** — declares JSON fields the LLM must produce.
  `src/distiller/parser.rs` enforces this; parser tests will catch drift.
- **`[prompt]`** — `system` + `user_template`.
- **`[prompt_overrides]`** (recipes only) — `extra_system` prepended to default's `system`.

## Don't

- Don't quote raw transcript content in the system prompt — keep prompts
  source-independent so they work for any session.
- Don't change `[output_schema]` without updating the parser. Tests will fail.
- Don't put secrets in here. They get version-controlled.

## Customizing

See `docs/CUSTOMIZING_PROMPTS.md`.
