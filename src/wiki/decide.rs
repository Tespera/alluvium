//! Threshold-based new-vs-update decision.

use anyhow::Result;

pub fn decide(
    _fact: &crate::extraction::ExtractedFact,
    _candidates: &[std::path::PathBuf],
) -> Result<super::TargetPage> {
    anyhow::bail!("wiki::decide::decide: not yet implemented (scaffold v0.1)")
}
