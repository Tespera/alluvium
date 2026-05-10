//! Decide which topic page each [`crate::extraction::ExtractedFact`] belongs to.
//!
//! v0.1 is exact-slug only: given a fact, compute the canonical path
//! `<alluvium_root>/wiki/{concepts|entities}/<page_slug>.md` and check
//! whether it exists. New vs existing is the merger's signal for "create
//! a fresh page" vs "merge into the live one".
//!
//! v0.2 will add embedding-based fuzzy matching and cross-type
//! resolution (per ADR-006).

pub mod decide;
pub mod index_scan;
pub mod locator;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Target topic page for a fact. Path encodes the type (under
/// `wiki/concepts/` or `wiki/entities/`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TargetPage {
    /// File exists on disk; vault::merger will load it and merge into.
    Existing(PathBuf),
    /// File does not exist; vault::merger will create it from a template.
    New(PathBuf),
}

impl TargetPage {
    pub fn path(&self) -> &std::path::Path {
        match self {
            TargetPage::Existing(p) | TargetPage::New(p) => p,
        }
    }

    pub fn is_new(&self) -> bool {
        matches!(self, TargetPage::New(_))
    }
}
