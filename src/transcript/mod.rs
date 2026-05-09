//! Transcript handling.
//!
//! Reads Claude Code's session JSONL, reconstructs the conversation flow,
//! merges in any PreCompact snapshots, and emits a structured
//! `ConversationData` for the distiller.

pub mod jsonl;
pub mod merge_snapshots;
pub mod metadata;
pub mod reconstruct;

use serde::{Deserialize, Serialize};

/// Output of the transcript pipeline. Consumed by [`crate::distiller`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationData {
    pub session_id: String,
    pub messages: Vec<Message>,
    pub metadata: metadata::SessionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub input: serde_json::Value,
    pub output: Option<String>,
}
