//! Claude Code hook stdin payload.
//!
//! When a Claude Code Plugin hook fires, the command receives a JSON payload
//! on **stdin** (not via environment variables — `$SESSION_ID` does not exist).
//!
//! Schema (verified against Claude Code Hooks Reference):
//!
//! ```json
//! {
//!   "session_id": "abc123",
//!   "transcript_path": "/path/to/transcript.jsonl",
//!   "cwd": "/current/working/directory",
//!   "permission_mode": "default",
//!   "hook_event_name": "SessionStart" | "PreCompact" | "Stop" | "SessionEnd" | ...
//! }
//! ```
//!
//! See ADR-011 in `docs/DECISIONS.md`.

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPayload {
    pub session_id: String,
    pub transcript_path: std::path::PathBuf,
    pub cwd: std::path::PathBuf,
    #[serde(default)]
    pub permission_mode: Option<String>,
    pub hook_event_name: String,
    /// Optional sub-agent fields (only present when running inside a sub-agent context).
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
}

/// Read and parse the hook payload from stdin.
pub fn read_from_stdin() -> Result<HookPayload> {
    anyhow::bail!("hook::payload::read_from_stdin: not yet implemented (scaffold v0.1)")
}
