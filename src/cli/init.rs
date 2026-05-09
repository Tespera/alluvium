//! `alluvium init` — interactive setup wizard.
//!
//! Walks the user through:
//!   1. Vault path
//!   2. Recipe (minimalist | dev-journal | verbose)
//!   3. Anthropic API key (stored in OS Keychain, see [`crate::config::secrets`])
//!   4. Plugin install (writes [`crate::hook::plugin_manifest`] entries)
//!   5. Dry-run preview of the most recent session
//!
//! See `docs/HOOKS.md` for the contract this command upholds when installing
//! the Claude Code plugin.

use anyhow::Result;

pub async fn run() -> Result<()> {
    anyhow::bail!("alluvium init: not yet implemented (scaffold v0.1)")
}
