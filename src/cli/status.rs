//! `alluvium status` — show recent archive activity.
//!
//! Reads `~/.local/share/alluvium/log/archive.jsonl` and prints the last N
//! archive runs: which session, how many topic pages were touched, any errors.

use anyhow::Result;

pub async fn run() -> Result<()> {
    anyhow::bail!("alluvium status: not yet implemented (scaffold v0.1)")
}
