# Alluvium

> Your Claude Code sessions flow downstream. Alluvium is what settles.

Alluvium auto-archives Claude Code sessions to your Obsidian vault. After each session, it distills the transcript into knowledge — not chat logs — and merges it into your existing topic pages.

You don't save your sessions. Your sessions save themselves.

## Status

🚧 **v0.1 in early scaffolding.** Not yet usable.

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

## Prior art

See [`docs/PRIOR_ART.md`](docs/PRIOR_ART.md). We learned specifically from `obsidian-second-brain`, `claude-obsidian`, and `cognee-integrations`.

## License

Dual-licensed under the [MIT License](LICENSE-MIT) and [Apache License 2.0](LICENSE-APACHE), at your option. This matches Rust ecosystem conventions.
