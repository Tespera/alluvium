//! From distiller output to a list of typed `ExtractedFact`s.
//!
//! Each fact is one bullet of knowledge with a target topic page. Decisions
//! about *which* page a fact belongs to live in [`crate::wiki`].

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFact {
    pub kind: FactKind,
    pub page_slug: String,
    pub page_title: String,
    pub summary: String,
    pub body_markdown: String,
    pub relations: Relations,
    pub evidence: Vec<Evidence>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FactKind {
    Entity,
    Concept,
    Decision,
    Gotcha,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Relations {
    pub uses: Vec<String>,
    pub used_by: Vec<String>,
    pub related: Vec<String>,
    pub supersedes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Evidence {
    pub from_message_index: usize,
    pub snippet: String,
}

pub fn extract(_distiller_output: crate::distiller::DistillerOutput) -> Result<Vec<ExtractedFact>> {
    anyhow::bail!("extraction::extract: not yet implemented (scaffold v0.1)")
}
