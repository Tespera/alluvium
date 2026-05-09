//! PreCompact hook handler.
//!
//! Runs in <100 ms. Snapshots the current transcript JSONL into the session
//! cache so that detail compressed away by Claude Code's compaction is not
//! lost before the eventual Stop-hook archive run.
//!
//! See `docs/HOOKS.md` § PreCompact.

use anyhow::Result;

pub async fn run() -> Result<()> {
    anyhow::bail!("alluvium pre-compact: not yet implemented (scaffold v0.1)")
}
