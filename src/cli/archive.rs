//! `alluvium archive --session <id>` — the Stop-hook entry point.
//!
//! The hook itself just spawns this process detached. This function runs the
//! full archive pipeline:
//!   1. Acquire file lock ([`crate::hook::lock`])
//!   2. Self-filter check ([`crate::hook::self_filter`])
//!   3. Merge PreCompact snapshots + final transcript ([`crate::transcript`])
//!   4. Distill ([`crate::distiller`])
//!   5. Extract facts ([`crate::extraction`])
//!   6. Locate target topic pages ([`crate::wiki`])
//!   7. Merge + atomic write each topic page ([`crate::vault`])
//!   8. Append to log.md, update index.md
//!   9. Record archive log entry
//!
//! See `docs/HOOKS.md` and `docs/ARCHITECTURE.md` for the data flow.

use anyhow::Result;

pub async fn run(_session_id: &str) -> Result<()> {
    anyhow::bail!("alluvium archive: not yet implemented (scaffold v0.1)")
}
