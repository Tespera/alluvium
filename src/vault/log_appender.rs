//! Append a single line to `wiki/log.md`.
//!
//! Format: `- HH:MM <session title> → [[touched]] [[pages]]`.
//! Append-only; existing lines are never modified.

use anyhow::Result;
use std::path::Path;

pub fn append(_log_md: &Path, _line: &str) -> Result<()> {
    anyhow::bail!("vault::log_appender::append: not yet implemented (scaffold v0.1)")
}
