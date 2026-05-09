//! `alluvium replay [<id>] [--since <duration>] [--all]` — re-distill old sessions.
//!
//! Common after editing `prompts/*.toml` recipes: re-run distillation against
//! prior sessions so the resulting wiki reflects the new style.
//!
//! Internally this is the same pipeline as [`super::archive`], but with the
//! transcript path resolved from a stored session id (or many).

use anyhow::Result;

pub async fn run(_session: Option<&str>, _since: Option<&str>, _all: bool) -> Result<()> {
    anyhow::bail!("alluvium replay: not yet implemented (scaffold v0.1)")
}
