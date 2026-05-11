//! `alluvium reindex` — regenerate `wiki/index.md` from on-disk topics.
//!
//! Self-heal command. Useful when:
//!   - lint --apply (in an earlier version) deleted pages but left the
//!     index stale
//!   - the user moved / renamed / hand-deleted topic files in Obsidian
//!   - the user wants to force-refresh the index without running an
//!     archive
//!
//! No LLM calls, no destructive ops — just walks the wiki and rewrites
//! `wiki/index.md` based on what currently exists.

use anyhow::{Context, Result};

use crate::cli::paths;
use crate::config;
use crate::vault;

pub async fn run() -> Result<()> {
    let cfg = config::load(&paths::config_file()?)
        .context("loading config; run `alluvium init` first if this is a fresh install")?;

    let alluvium_root = cfg.default.vault_path.join(&cfg.default.alluvium_subdir);
    if !alluvium_root.join("wiki").exists() {
        println!(
            "reindex: no wiki under {} yet — nothing to do.",
            alluvium_root.display()
        );
        return Ok(());
    }

    vault::index_updater::update(&alluvium_root)
        .with_context(|| format!("regenerating index.md under {}", alluvium_root.display()))?;
    println!(
        "reindex: rebuilt {}/wiki/index.md from current on-disk topics.",
        alluvium_root.display()
    );
    Ok(())
}
