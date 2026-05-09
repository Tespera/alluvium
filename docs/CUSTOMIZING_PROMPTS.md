# Customizing Prompts

Alluvium's distillation behavior is controlled by `*.toml` prompt files in
`<config>/prompts/` (typically `~/.config/alluvium/prompts/` on Linux,
`~/Library/Application Support/dev.alluvium.alluvium/prompts/` on macOS).
You can edit them anytime — Alluvium re-reads them on every archive.

## File layout

```
<config>/prompts/
├── distill.toml        # the base prompt — system + user template
├── merge.toml          # placeholder (not yet used; reserved for v0.2)
├── classify.toml       # placeholder (not yet used)
└── recipes/
    ├── dev-journal.toml    # default: first-person past-tense narrative
    ├── minimalist.toml     # terse, conclusions only
    └── verbose.toml        # full pedagogical write-up with code snippets
```

`alluvium init` populates this directory from compiled-in defaults. If you
delete a file, re-run `alluvium init` to restore. If you _edit_ a file,
init won't overwrite your changes.

## How recipes work

A recipe inherits from `distill.toml` and overrides specific things:

```toml
[meta]
name = "minimalist"
description = "..."
inherits = "default"
model = "claude-haiku-4-5"   # optional: override base model

[overrides]
max_facts_per_session = 5    # cap on number of facts emitted
max_body_chars = 400         # advisory cap on each fact's body length

[prompt_overrides]
# This is APPENDED to the base system prompt.
extra_system = """
RECIPE OVERRIDE — minimalist:
- Style: terse, conclusions only.
- ...
"""
```

Pick a recipe in `~/.config/alluvium/config.toml`:

```toml
[default]
recipe = "minimalist"
```

## Common customizations

### Change the recipe style

Edit `<config>/prompts/recipes/dev-journal.toml`'s `extra_system` to taste.
For example, to make Alluvium write in second-person present tense:

```toml
[prompt_overrides]
extra_system = """
RECIPE OVERRIDE — dev-journal (custom):
- Style: second-person present tense ("you decide to use X").
- ...
"""
```

### Make the LLM produce more / fewer facts

Adjust `max_facts_per_session` in `[overrides]`. The LLM will see this
constraint in its prompt and try to respect it.

### Switch to Sonnet for higher quality

```toml
# In recipes/dev-journal.toml or distill.toml
[meta]
model = "claude-sonnet-4-6"
```

Increases cost ~5× but produces longer, more thoughtful body content.

### Tune what gets into the prompt

`[byte_caps]` in `distill.toml` controls how much of each transcript field
the LLM sees. If you want more detail per turn, raise `tool_result` or
`assistant_message`:

```toml
[byte_caps]
tool_use = 4096
tool_result = 16384      # was 8192
user_message = 8192
assistant_message = 32768  # was 16384
```

This increases token usage and cost; use only if you've seen distillations
miss important details from truncated tool outputs.

## ⚠️ Warning: HTML comment markers

Each Alluvium-written paragraph in a topic page is wrapped in:

```markdown
<!-- alluvium:fact id=abc12345 -->
... fact body ...
<!-- alluvium:end -->
```

**Don't delete these markers.** They're how Alluvium identifies its own
content for in-place updates (per ADR-009). If you delete them:

- Next archive treats the content as user-written → keeps it.
- Same archive ALSO appends a fresh copy of that fact → you get duplicates.

If you want to "claim" content as yours and prevent future overwrites,
**move it outside the markers** (above or below the block). Anything
outside `<!-- alluvium:fact ... -->` ... `<!-- alluvium:end -->` is left
alone forever.

## Testing your prompt changes

After editing a prompt or recipe:

```bash
alluvium dry-run
```

This re-runs the full distillation on your most recent session but
prints the resulting JSON to stdout instead of writing to your vault. You
can iterate quickly without polluting the vault.

Once you're happy, the next real Claude Code session end will use your
updated prompt automatically.

## Restoring defaults

If you've broken a prompt and want to start over:

```bash
rm ~/.config/alluvium/prompts/recipes/dev-journal.toml
alluvium init
```

`init` will copy the bundled default back into place. (It won't overwrite
files that exist, so removal first is required.)

## Schema reference

The full TOML schema each prompt file uses lives in
[`src/distiller/prompt.rs`](../src/distiller/prompt.rs)'s `BaseFile` and
`RecipeFile` structs. The output JSON schema the LLM is told to produce
is documented in `prompts/distill.toml`'s system prompt and enforced by
[`src/distiller/parser.rs`](../src/distiller/parser.rs).

If you change the system prompt's "Output format" section, you may need
to update the parser too — but for casual style tweaks, the JSON shape
should stay constant.
