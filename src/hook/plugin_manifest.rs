//! Generate and validate `.claude-plugin/plugin.json`.
//!
//! Used by `alluvium init` to install Alluvium as a Claude Code plugin
//! (rather than mutating `~/.claude/settings.json`). See ADR-004 in
//! `docs/DECISIONS.md`.

use anyhow::Result;

pub fn install() -> Result<()> {
    anyhow::bail!("hook::plugin_manifest::install: not yet implemented (scaffold v0.1)")
}

pub fn uninstall() -> Result<()> {
    anyhow::bail!("hook::plugin_manifest::uninstall: not yet implemented (scaffold v0.1)")
}
