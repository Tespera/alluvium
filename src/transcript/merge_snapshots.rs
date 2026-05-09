//! Merge PreCompact snapshots with the final session JSONL.
//!
//! When a long session triggers context compaction, the JSONL only retains
//! the post-compaction summary. Snapshots saved by the PreCompact hook
//! preserve the original detail; this module stitches them together.

use anyhow::Result;
use std::path::Path;

pub fn merge(_final_jsonl: &Path, _snapshots_dir: &Path) -> Result<Vec<serde_json::Value>> {
    anyhow::bail!("transcript::merge_snapshots::merge: not yet implemented (scaffold v0.1)")
}
