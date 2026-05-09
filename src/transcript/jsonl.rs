//! Streaming JSONL reader for `~/.claude/projects/<dir>/<session>.jsonl`.

use anyhow::Result;
use std::path::Path;

pub fn read(_path: &Path) -> Result<Vec<serde_json::Value>> {
    anyhow::bail!("transcript::jsonl::read: not yet implemented (scaffold v0.1)")
}
