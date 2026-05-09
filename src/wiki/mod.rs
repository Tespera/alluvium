//! Decide which topic page each [`crate::extraction::ExtractedFact`] belongs to.
//!
//! v0.1 strategy: grep the vault for the slug + fuzzy title match against
//! existing concept/entity pages. If the match score crosses a threshold,
//! return that page; otherwise create a new one.
//!
//! v0.2 will add embedding-based similarity (see ADR-006).

pub mod decide;
pub mod locator;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TargetPage {
    Existing(std::path::PathBuf),
    NewConcept(String),
    NewEntity(String),
}
