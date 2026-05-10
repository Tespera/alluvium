# Changelog

All notable changes to Alluvium are documented here. The format is loosely
based on [Keep a Changelog](https://keepachangelog.com/), and the project
adheres to [Semantic Versioning](https://semver.org/) once it ships 1.0.

## [Unreleased] — v0.1.0

A genuinely working v0.1: the full Karpathy LLM-Wiki algorithm —
*compounding, deduplicating, lint-able* — is implemented end-to-end,
not just the directory layout.

### Karpathy fidelity (the make-or-break work)

See [docs/LLM_WIKI_DOCTRINE.md](docs/LLM_WIKI_DOCTRINE.md) for the
5-principle product bible; this section is the implementation status.

- **Wiki-aware ingest** (Doctrine principle 2). Each archive scans the
  vault first and feeds the existing topic index (slug + title +
  summary, ≤400 chars) into the distill prompt. The LLM is explicitly
  instructed to *reuse* slugs when a fact concerns an existing topic
  rather than mint a parallel one. Without this, the wiki accumulates
  parallel-universe duplicates (the v0.1.0-alpha behavior).
- **Episode filter** (Doctrine principle 5.1). Distill prompt now
  enumerates "events / current-state snapshots / TODOs / personal
  feelings" as DO-NOT-extract — those belong in `log.md`, not
  `concepts/`.
- **Lint command** (Doctrine principle 3 — mandatory): `alluvium lint`
  scans the wiki, scores every page-pair on five independent axes
  (slug Levenshtein, title Levenshtein, bigram Jaccard, CJK-char-set
  Jaccard, ASCII-word Jaccard), sends candidates to the LLM for a
  MERGE/KEEP decision, and (with `--apply`) rewrites winners + deletes
  losers + appends a `## [date] lint | merged X → Y` line to log.md.
  Cross-language clusters where slugs and titles are in different
  scripts are caught via the multi-axis Jaccard pre-filter.
- **Vault language config**: optional `vault_language` field in
  `config.toml` steers slug-language consistency across sessions.

### CLI (11 subcommands)
- `init`, `archive`, `replay`, `status`, `dry-run`, `consolidate`,
  `lint`, `session-start`, `pre-compact`, `session-end`, `uninstall`.
- `consolidate <slug>` is a real implementation (was a placeholder in
  the alpha): reads a topic page, feeds every alluvium:fact block to
  the LLM, replaces them with one consolidated block. Frontmatter and
  content above the first / below the last block survive.
- `lint [--apply]` is documented above.

### Hook plumbing
- 4 Claude Code Plugin hooks (SessionStart / PreCompact / Stop /
  SessionEnd) declared in `.claude-plugin/plugin.json`.
- Detached spawn (Stop hook returns within ~100 ms while archive runs
  in the background, per ADR-008).
- claude-cli backend has a 180s timeout + `kill_on_drop` to bound the
  observed pathology where `claude -p` occasionally hangs and returns
  truncated stdout.

### Multi-backend distillation
- 5 backends: `claude-cli` (default — uses the user's existing Claude
  Code OAuth), `anthropic` (SSE streaming), `openai`, `deepseek`,
  `gemini`. Auto-detect order: claude-cli on PATH → ANTHROPIC_API_KEY
  → OPENAI_API_KEY → DEEPSEEK_API_KEY → GEMINI_API_KEY.

### Prompts (5 bundled)
- `distill.toml` — wiki-aware ingest with episode filter.
- `consolidate.toml` — single-page block consolidation.
- `lint.toml` — pairwise MERGE/KEEP decision.
- `recipes/{dev-journal,minimalist,verbose}.toml` — style overlays.

### Vault model
- Karpathy layout: `raw/`, `wiki/concepts/`, `wiki/entities/`,
  `wiki/log.md`, `wiki/index.md`.
- Two-layer merge per topic page: HTML-comment block markers (ADR-009)
  + three-way frontmatter merge (ADR-014). User edits both inside and
  outside Alluvium-managed regions are preserved.
- Atomic writes (tempfile + rename); cross-process file lock (flock
  via fs2); cross-platform paths via the `directories` crate (ADR-010).
- Wiki-locator with normalized-Levenshtein fuzzy matching (slug + title)
  catches case / suffix drift before the new-page path fires.

### `--debug` mode
- `alluvium --debug archive ...` dumps every pipeline stage's
  intermediate JSON (rendered prompt, raw LLM output, parsed output)
  to `<data_dir>/debug/<session>/` for postmortem.

### Tests + CI
- 327+ unit tests + 18 end-to-end tests covering: archive_new /
  archive_update / user_edit_preserved / consolidate / lint /
  wiki_aware_ingest / replay / replay_bulk / pre_compact / dry_run /
  hook_idempotent / hook_returns_fast / concurrent_sessions /
  self_reference. CI runs `cargo test`, `cargo clippy -D warnings`,
  `cargo fmt --check` on Ubuntu + macOS.

### Known limitations (deferred to v0.2)
- Plugin install is still a manual `claude plugin marketplace add` +
  `claude plugin install` step. The Homebrew formula is written
  (`extra/Formula/alluvium.rb`) but the tap repo is not yet published.
- Genuine cross-vocabulary semantic duplicates (same topic, completely
  different prose with no shared CJK chars or ASCII words) require
  embeddings, which v0.1 doesn't ship. Surface heuristics catch most
  cases (verified against a real ~120-page Chinese-heavy vault) but
  embeddings are needed for the long tail.
- Single profile only; multi-profile config layout reserved but
  ignored (ADR-005).
- Only macOS keychain integration is fully tested in CI; Linux
  SecretService and Windows Credential Manager are best-effort.
- Sub-agent transcripts are filtered out of distillation entirely. v0.2
  may surface them when the user explicitly opts in.

### What's NOT in scope ever (per ADR / V01_SCOPE)
- Cross-client (Cursor, Claude Desktop, Codex) — Alluvium is Claude
  Code only by design. Other clients can pipe sessions via custom
  scripts but won't be auto-detected.
- Cloud sync / multi-device — vault is the user's file tree; sync via
  iCloud/Syncthing/git is the user's choice.
- Team / multi-user — Alluvium is a personal PKM tool.

### Cost (default Haiku 4.5)
- ~$0.06 per Claude Code session (50K input / 2K output average).
- ~$9/month for 5 sessions/day.
- Switch to Sonnet via `~/.config/alluvium/config.toml` for ~5× cost
  with better notes; switch to `minimalist` recipe to halve.

### Migration
First release; no migration needed.
