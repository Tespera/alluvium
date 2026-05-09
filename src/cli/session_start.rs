//! SessionStart hook handler.
//!
//! Runs in <50 ms. Reads stdin payload, reads config, performs self-filter
//! check via [`crate::hook::self_filter::should_skip`], writes per-session
//! `resolved.json` to `<cache>/sessions/<id>/`.
//!
//! See `docs/HOOKS.md` § SessionStart.

use anyhow::Result;

pub async fn run() -> Result<()> {
    anyhow::bail!("alluvium session-start: not yet implemented (scaffold v0.1)")
}
