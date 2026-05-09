//! Convert raw JSONL events into a clean conversation flow.
//!
//! Filters out sub-agent noise, merges tool-call request/response pairs,
//! collapses streaming chunks. Output is `Vec<Message>`.

use anyhow::Result;

use super::Message;

pub fn reconstruct(_events: Vec<serde_json::Value>) -> Result<Vec<Message>> {
    anyhow::bail!("transcript::reconstruct::reconstruct: not yet implemented (scaffold v0.1)")
}
