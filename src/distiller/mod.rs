//! LLM distillation layer.
//!
//! Multi-backend: claude-cli (default, uses user's Claude Code OAuth),
//! Anthropic, OpenAI, DeepSeek, Gemini. See [`backend`] for the trait and
//! [`backends`] for concrete implementations. [`selection`] picks one based
//! on config + auto-detection.

pub mod backend;
pub mod backends;
pub mod budget;
pub mod parser;
pub mod prompt;
pub mod selection;

use serde::{Deserialize, Serialize};

/// Input to the distiller — the data we want shaped into a prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillerInput {
    pub conversation: crate::transcript::ConversationData,
    pub recipe_name: String,
    /// Existing topic pages already in the wiki — fed to the LLM so it
    /// can reuse slugs / update existing pages instead of minting parallel
    /// duplicates. Empty on a fresh vault.
    /// See [LLM_WIKI_DOCTRINE](../../docs/LLM_WIKI_DOCTRINE.md) principle 2.
    #[serde(default)]
    pub existing_topics: Vec<crate::wiki::index_scan::TopicEntry>,
    /// User-configured vault language ("zh" / "en" / "ja" / ...). Steers
    /// the LLM toward consistent slug language across sessions. `None`
    /// means "auto-detect from transcript dominant language".
    #[serde(default)]
    pub vault_language: Option<String>,
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

/// Token counts surfaced by an LLM backend (when available). `claude -p`
/// in plain-text mode does not surface these and reports `None`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}
