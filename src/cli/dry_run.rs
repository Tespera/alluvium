//! `alluvium dry-run` — distill the most recent session without writing.
//!
//! Used during `init` for the "preview effect" step, and any time a user wants
//! to test a prompt change before committing.

use anyhow::Result;

pub async fn run() -> Result<()> {
    anyhow::bail!("alluvium dry-run: not yet implemented (scaffold v0.1)")
}
