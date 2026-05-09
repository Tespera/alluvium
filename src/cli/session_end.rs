//! SessionEnd hook handler.
//!
//! Removes the per-session cache directory (`<cache>/sessions/<id>/`). The
//! detached archive worker (spawned by Stop hook) does NOT depend on this
//! directory after it starts — it reads transcript paths from
//! `~/.claude/projects/`, not from cache — so cleanup here is safe even
//! if archive is still running.
//!
//! Must return within 50 ms (per docs/HOOKS.md).

use anyhow::Result;

use crate::cli::paths;
use crate::hook::payload;

pub async fn run() -> Result<()> {
    let p = payload::read_from_stdin()?;
    let dir = paths::session_cache_dir(&p.session_id)?;
    if dir.exists() {
        // Best-effort. If removal fails (permissions, etc.), log but don't fail.
        if let Err(err) = std::fs::remove_dir_all(&dir) {
            tracing::warn!(
                dir = %dir.display(),
                error = %err,
                "session-end cleanup failed (non-fatal)"
            );
        }
    }
    Ok(())
}
