//! Self-reference filter.
//!
//! If the user is using Claude Code to develop Alluvium itself, that session
//! must NOT be archived (would create recursive notes about archive logic).
//!
//! Decision: SessionStart compares cwd against the Alluvium dev directory
//! (auto-detected) and any user-configured `skip_paths`. Result lands in
//! `resolved.json` as `skip_reason`. Subsequent hooks no-op when set.

use anyhow::Result;
use std::path::Path;

pub fn should_skip(_cwd: &Path, _skip_paths: &[std::path::PathBuf]) -> Result<Option<String>> {
    anyhow::bail!("hook::self_filter::should_skip: not yet implemented (scaffold v0.1)")
}
