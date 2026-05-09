//! Diff-based merge: combine new content into an existing topic page,
//! preserving the user's hand-edits.
//!
//! Strategy: track sections we wrote previously; on merge, only replace
//! those tracked sections. Non-tracked content (user additions) is kept verbatim.

use anyhow::Result;

pub fn merge_into_page(
    _existing: &str,
    _new_fact: &crate::extraction::ExtractedFact,
) -> Result<String> {
    anyhow::bail!("vault::merger::merge_into_page: not yet implemented (scaffold v0.1)")
}
