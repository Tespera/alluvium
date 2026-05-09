# Changelog

All notable changes to Alluvium are documented here. The format is loosely
based on [Keep a Changelog](https://keepachangelog.com/), and the project
adheres to [Semantic Versioning](https://semver.org/) once it ships 1.0.

## [Unreleased] — v0.1.0 alpha

The first end-to-end functional cut. The full archive pipeline runs:
hook payload → distill → merge into Karpathy-wiki vault → audit log.

### What works
- 10 CLI subcommands: `init`, `archive`, `replay`, `status`, `dry-run`,
  `consolidate`, `session-start`, `pre-compact`, `session-end`, `uninstall`.
- 4 Claude Code Plugin hooks (SessionStart / PreCompact / Stop / SessionEnd)
  declared in `.claude-plugin/plugin.json`.
- Detached spawn so the Stop hook returns within 100 ms while archive
  runs in the background (per ADR-008).
- Full transcript handling: `~/.claude/projects/*/*.jsonl` parsing,
  PreCompact snapshot merging, sub-agent (sidechain) filtering,
  polymorphic content reconstruction.
- Anthropic Messages API client (no third-party SDK; pure reqwest).
- 3 prompt recipes shipped (`dev-journal`, `minimalist`, `verbose`)
  with externalized `prompts/*.toml` files; users can edit without
  recompiling.
- Karpathy-wiki vault layout (`raw/`, `wiki/concepts/`, `wiki/entities/`,
  `wiki/log.md`, `wiki/index.md`).
- Two-layer merge per topic page: HTML-comment block markers (ADR-009)
  for body content + three-way merge for frontmatter list fields
  (ADR-014). User edits — both inside and outside Alluvium-managed
  regions where applicable — are preserved.
- Atomic writes (tempfile + rename) prevent torn writes under concurrent
  archive runs.
- Cross-process file lock (flock via fs2) serializes archive workers.
- Cross-platform paths via the `directories` crate (ADR-010).
- API key in OS keychain (macOS) or `$ANTHROPIC_API_KEY` env-var fallback.
- Idempotent `index.md` regeneration: O(N) frontmatter-only scan.
- Append-only `log.md` with per-date sections; tolerates user-handwritten
  notes anywhere.
- 278 unit tests (`cargo test --lib`); CI runs on Ubuntu + macOS.

### Known limitations (deferred to v0.2)
- Plugin install is a manual `claude plugin install <path>` step. v0.1
  prints the command from `init`; v0.2 will automate.
- `consolidate` is a placeholder — full LLM-driven topic-page rewriting
  for append-only-drift defense is v0.2 (ADR per V01_SCOPE).
- No embedding-based fact dedup yet; v0.1 is exact-slug matching only
  (ADR-006).
- Single profile only; multi-profile config layout reserved but
  ignored (ADR-005).
- No automatic Homebrew distribution yet — install via `cargo install
  --path .` from the repo.
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
