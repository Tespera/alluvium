//! Atomic file write: temp file + rename.

use anyhow::Result;
use std::path::Path;

pub fn write_atomic(_path: &Path, _content: &str) -> Result<()> {
    anyhow::bail!("vault::writer::write_atomic: not yet implemented (scaffold v0.1)")
}
