//! LLM distillation layer.
//!
//! Calls the Anthropic API with a rendered prompt template and parses the
//! structured response into `DistillerOutput`. The output is then handed to
//! [`crate::extraction`] for fact-level processing.

pub mod budget;
pub mod client;
pub mod parser;
pub mod prompt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillerInput {
    pub conversation: crate::transcript::ConversationData,
    pub recipe_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillerOutput {
    pub title: String,
    pub tags: Vec<String>,
    pub raw_facts_json: serde_json::Value,
}
