//! Per-archive audit trail.
//!
//! Writes one line of JSONL to `~/.local/share/alluvium/log/archive.jsonl`
//! per archive run. Read by [`status`] to render the recent-activity table.

pub mod status;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveLogEntry {
    pub session_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub finished_at: chrono::DateTime<chrono::Utc>,
    pub touched_pages: Vec<std::path::PathBuf>,
    pub error: Option<String>,
}
