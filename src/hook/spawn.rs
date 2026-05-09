//! Detached subprocess spawning.
//!
//! The Stop hook spawns `alluvium archive` and exits within 100 ms; the child
//! continues in the background. On Unix we put the child in its own process
//! group so it survives the parent's exit.
//!
//! Pattern borrowed from cognee-integrations' `_spawn_detached_sync()`.
//! See ADR-008 in `docs/DECISIONS.md`.

use anyhow::Result;

pub fn spawn_archive_detached(_session_id: &str) -> Result<()> {
    anyhow::bail!("hook::spawn::spawn_archive_detached: not yet implemented (scaffold v0.1)")
}
