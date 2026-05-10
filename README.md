# Alluvium

> Your Claude Code sessions flow downstream. Alluvium is what settles.

Alluvium auto-archives Claude Code sessions to your Obsidian vault. After each session, it distills the transcript into knowledge — not chat logs — and merges it into your existing topic pages.

You don't save your sessions. Your sessions save themselves.

## Status

🟡 **v0.1.0 alpha** — end-to-end pipeline functional; manual install.
Real-world usage feedback welcome. See [`CHANGELOG.md`](CHANGELOG.md) for
what works and what's deferred to v0.2.

## Quickstart (v0.1 alpha)

```bash
# 1. Install (pick one — the Homebrew tap is prepared but not yet published;
#    once the v0.1.0 GitHub release lands, `brew tap Tespera/alluvium` will work.
#    Until then, build from source.)
cargo install --git https://github.com/Tespera/alluvium

# or, from a local checkout:
git clone https://github.com/Tespera/alluvium
cd alluvium
cargo install --path .

# 2. Configure: vault path, recipe, API key
alluvium init

# 3. Register the Claude Code plugin (init prints this command)
claude plugin install /path/to/alluvium

# 4. (Optional) Backfill the past week of sessions
alluvium replay --since 7d
```

After that, just keep using Claude Code. Each session ends → distilled
knowledge appears in your vault, **integrated into existing topic pages**
(not piled up as new files). The LLM reads your wiki's existing-topics
index before extracting facts, so a session about "atomic file writes"
adds to the existing `atomic-write` page instead of creating a parallel
`atomic-file-write` / `file-atomic-write` / `三向-原子写盘` zoo.

Run `alluvium status` to see recent archives. Run `alluvium dry-run` to
preview the next distillation without writing. Run `alluvium lint`
periodically — it scans the wiki for near-duplicate pages that snuck
through and asks the LLM whether to merge them; pass `--apply` to
actually do it.

## Concept

Most Claude + Obsidian tools either embed Claude inside Obsidian (you ask it to do things) or expose your vault to Claude via MCP (Claude reads/writes when prompted). Alluvium does neither.

It watches Claude Code sessions end, distills the conversation, and updates a [Karpathy-style LLM wiki](https://gist.github.com/karpathy/442a6bf555914893e9891c11519de94f) in your vault.

**Passive.** No slash command, no sidebar, no "remember to save." Hook-driven; you forget it's there.

**Compounding.** Each session merges into existing topic pages instead of creating yet another timestamped note. Your knowledge converges instead of accumulating.

## Architecture

- Installed as a [Claude Code Plugin](https://docs.claude.com/en/docs/claude-code/plugins) (`.claude-plugin/plugin.json`)
- 4 hooks: `SessionStart` / `PreCompact` / `Stop` / `SessionEnd`
- Distillation via Anthropic API with externalized prompt templates (`prompts/*.toml`)
- Vault layout follows Karpathy's LLM Wiki pattern (`raw/`, `wiki/`, `log.md`, `index.md`)
- Single Rust binary, distributed via Homebrew

See [`docs/`](docs/) for design details.

## Cost

Alluvium calls the Anthropic API once per Claude Code session. Default model
is **Claude Haiku 4.5** (cheapest tier).

Rough estimate (default `dev-journal` recipe, ~50-message session):

| Granularity         | Tokens                       | Cost      |
| ------------------- | ---------------------------- | --------- |
| Per session         | ~50K input / ~2K output      | **~$0.06** |
| Per day (5 sessions) | —                           | **~$0.30** |
| Per month           | —                            | **~$9**   |

Switch to Sonnet (better notes, ~5× cost) or another model in
`~/.config/alluvium/config.toml`. The `verbose` recipe roughly doubles cost;
`minimalist` halves it.

## Prior art

See [`docs/PRIOR_ART.md`](docs/PRIOR_ART.md). We learned specifically from `obsidian-second-brain`, `claude-obsidian`, and `cognee-integrations`.

## License

Dual-licensed under the [MIT License](LICENSE-MIT) and [Apache License 2.0](LICENSE-APACHE), at your option. This matches Rust ecosystem conventions.
