//! SessionStart hook handler.
//!
//! Runs in <50 ms. Reads config, performs self-filter check, writes per-session
//! `resolved.json` to `~/.cache/alluvium/sessions/<id>/`.
//!
//! See `docs/HOOKS.md` § SessionStart.

use anyhow::Result;

pub async fn run() -> Result<()> {
    anyhow::bail!("alluvium session-start: not yet implemented (scaffold v0.1)")
}
