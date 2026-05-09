//! Incremental update of `wiki/index.md`.
//!
//! Reads each topic page's frontmatter only (NOT body) to keep the operation
//! cheap even at thousands of pages. Regenerates the index sections from
//! the gathered metadata.

use anyhow::Result;
use std::path::Path;

pub fn update_incremental(_vault: &Path) -> Result<()> {
    anyhow::bail!("vault::index_updater::update_incremental: not yet implemented (scaffold v0.1)")
}
