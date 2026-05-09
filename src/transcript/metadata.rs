//! Extract session metadata: cwd, session id, time range, model, token totals.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    pub cwd: std::path::PathBuf,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub model: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

pub fn extract(_events: &[serde_json::Value]) -> Result<SessionMetadata> {
    anyhow::bail!("transcript::metadata::extract: not yet implemented (scaffold v0.1)")
}
