//! File lock to serialize concurrent archive runs.
//!
//! Two Claude Code sessions ending nearly simultaneously can otherwise race
//! on the same vault topic page. Uses `fs2` exclusive flock on
//! `~/.cache/alluvium/lock`.

use anyhow::Result;

pub struct ArchiveLock {
    _file: std::fs::File,
}

pub fn acquire() -> Result<ArchiveLock> {
    anyhow::bail!("hook::lock::acquire: not yet implemented (scaffold v0.1)")
}
