//! `alluvium consolidate <slug>` — manually rewrite a fragmented topic
//! page into a single consolidated `alluvium:fact` block.
//!
//! Workflow:
//!   1. Resolve `<vault>/Alluvium/wiki/{concepts,entities}/<slug>.md`
//!   2. Read it; extract every Alluvium fact-block; if <2 blocks, no-op.
//!   3. Render the `consolidate.toml` prompt with all block bodies.
//!   4. Call the configured LLM backend.
//!   5. Replace ALL existing blocks with one new block holding the
//!      consolidated body. Frontmatter and content outside the
//!      envelope (above the first block, below the last) survive.
//!   6. Atomic write back.

use anyhow::{Context, Result};

use crate::cli::paths;
use crate::config;
use crate::consolidate;
use crate::distiller;

pub async fn run(slug: &str) -> Result<()> {
    let cfg = config::load(&paths::config_file()?)
        .context("loading config; run `alluvium init` first if this is a fresh install")?;

    let alluvium_root = cfg.default.vault_path.join(&cfg.default.alluvium_subdir);

    let path = consolidate::find_page(&alluvium_root, slug).with_context(|| {
        format!(
            "no topic page found for slug {slug:?} under {}/wiki/{{concepts,entities}}/",
            alluvium_root.display()
        )
    })?;

    let backend =
        distiller::selection::pick(cfg.default.backend.as_deref(), cfg.default.model.as_deref())?;
    tracing::info!(
        slug = %slug,
        backend = %backend.kind(),
        "consolidate: starting"
    );

    let prompt_path = paths::prompts_dir()?.join("consolidate.toml");

    match consolidate::consolidate_page(&path, backend.as_ref(), &prompt_path).await? {
        Some(out) => {
            println!(
                "consolidate: collapsed {} fact-blocks in {}",
                out.fragments_collapsed,
                out.touched_path.display()
            );
        }
        None => {
            println!(
                "consolidate: nothing to do — {} has fewer than 2 fact-blocks.",
                path.display()
            );
        }
    }
    Ok(())
}
