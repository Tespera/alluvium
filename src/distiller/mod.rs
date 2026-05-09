//! LLM distillation layer.
//!
//! Calls the Anthropic API with a rendered prompt and parses the structured
//! response into [`DistillerOutput`]. The output (a list of typed
//! [`crate::extraction::ExtractedFact`]s) is what wiki + vault modules
//! consume to build / merge topic pages.
//!
//! Module roles:
//! - [`client`]  — HTTP transport (Anthropic Messages API)
//! - [`prompt`]  — load `prompts/*.toml`, render with minijinja
//! - [`parser`]  — Anthropic response Value → [`DistillerOutput`]
//! - [`budget`]  — pre-prompt byte caps for transcript fields

pub mod budget;
pub mod client;
pub mod parser;
pub mod prompt;

use serde::{Deserialize, Serialize};

/// Input to the distiller — the data we want shaped into a prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillerInput {
    pub conversation: crate::transcript::ConversationData,
    pub recipe_name: String,
}

/// What a full distillation yields: session-level title + tags + a list of
/// typed extracted facts ready for the wiki layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillerOutput {
    pub title: String,
    pub tags: Vec<String>,
    pub facts: Vec<crate::extraction::ExtractedFact>,
    pub usage: Option<TokenUsage>,
}

/// Token counts surfaced by the Anthropic API. Used for cost tracking and
/// audit-log entries; treated as best-effort (older transcripts may omit).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}
