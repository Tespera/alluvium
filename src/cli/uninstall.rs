//! `alluvium uninstall` — remove the Claude Code plugin (config preserved).
//!
//! Inverse of the plugin-install step in [`super::init`]. Does NOT delete:
//!   - `~/.config/alluvium/config.toml`
//!   - Anthropic API key in Keychain
//!   - Any vault content
//!
//! For full removal the user runs `alluvium uninstall --purge` (v0.2).

use anyhow::Result;

pub async fn run() -> Result<()> {
    anyhow::bail!("alluvium uninstall: not yet implemented (scaffold v0.1)")
}
