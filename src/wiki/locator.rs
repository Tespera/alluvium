//! Locate candidate existing topic pages in the vault.
//!
//! Grep for the slug, list near matches by title fuzzy distance.

use anyhow::Result;
use std::path::{Path, PathBuf};

pub fn candidates(_vault: &Path, _slug: &str) -> Result<Vec<PathBuf>> {
    anyhow::bail!("wiki::locator::candidates: not yet implemented (scaffold v0.1)")
}
