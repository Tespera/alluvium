//! SessionEnd hook handler.
//!
//! Runs in <50 ms. Removes per-session cache directory.
//! Detached `archive` workers spawned by the Stop hook are unaffected — they
//! read transcripts from `~/.claude/projects/`, not from the cache.
//!
//! See `docs/HOOKS.md` § SessionEnd.

use anyhow::Result;

pub async fn run() -> Result<()> {
    anyhow::bail!("alluvium session-end: not yet implemented (scaffold v0.1)")
}
