//! Alluvium: auto-archive Claude Code sessions to your Obsidian vault.
//!
//! Architectural source-of-truth lives in `CLAUDE.md` and `docs/` at the repo
//! root. Read those first if you are an AI assistant entering this codebase.
//!
//! Module map (also in `docs/ARCHITECTURE.md`):
//!
//! - [`cli`]         · subcommand dispatch
//! - [`config`]      · `~/.config/alluvium/config.toml` + Keychain
//! - [`hook`]        · plugin manifest, file lock, self-filter, detached spawn
//! - [`transcript`]  · `~/.claude/projects/*.jsonl` parsing
//! - [`distiller`]   · Anthropic API client + prompt rendering
//! - [`extraction`]  · transcript → ExtractedFacts
//! - [`wiki`]        · decide which topic page each fact belongs to
//! - [`vault`]       · atomic write, frontmatter merge, log/index updaters
//! - [`consolidate`] · LLM-driven topic-page rewrites
//! - [`log`]         · per-archive audit trail

pub mod audit;
pub mod cli;
pub mod config;
pub mod consolidate;
pub mod distiller;
pub mod extraction;
pub mod hook;
pub mod lint;
pub mod log;
pub mod rewrite;
pub mod transcript;
pub mod vault;
pub mod wiki;

use std::sync::OnceLock;

/// Process-wide `--debug` flag. Set once at CLI parse time. When true,
/// `archive` dumps each pipeline stage's intermediate artifact (rendered
/// prompt, raw LLM output, parsed output) to `<data_dir>/debug/<session>/`
/// for post-mortem inspection.
static DEBUG_FLAG: OnceLock<bool> = OnceLock::new();

pub fn set_debug_flag(value: bool) {
    let _ = DEBUG_FLAG.set(value);
}

pub fn debug_enabled() -> bool {
    *DEBUG_FLAG.get().unwrap_or(&false)
}
