//! Defends against append-only drift in the wiki.
//!
//! v0.1: manual command (`alluvium consolidate`) selects fragmented topic
//! pages (heuristics: high source count, high body length, last-update age)
//! and asks the LLM to rewrite them tighter while preserving all facts.
//!
//! v0.2: scheduled (cron / launchd) runs.

use anyhow::Result;

pub async fn run(_vault: std::path::PathBuf) -> Result<()> {
    anyhow::bail!("consolidate::run: not yet implemented (scaffold v0.1)")
}
