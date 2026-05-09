//! `alluvium archive` — the Stop-hook entry point (also reachable manually).
//!
//! Two invocation modes:
//!
//! - **Hook mode** (no `--session` arg): reads the Claude Code Stop hook
//!   payload from stdin (JSON with `session_id`, `transcript_path`, `cwd`,
//!   `hook_event_name`, etc.) via [`crate::hook::payload`]. The Stop hook in
//!   `.claude-plugin/plugin.json` invokes this mode.
//! - **Manual mode** (`--session <id>`): re-archive a specific session by id.
//!   `crate::cli::replay` calls into this mode internally.
//!
//! Pipeline (after either mode resolves session metadata):
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

pub async fn run(_session_id: Option<&str>) -> Result<()> {
    anyhow::bail!("alluvium archive: not yet implemented (scaffold v0.1)")
}
