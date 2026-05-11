//! `alluvium rewrite <slug>` or `alluvium rewrite --all` — reframe
//! episodic-prose topic pages as timeless.

use anyhow::{Context, Result};

use crate::cli::paths;
use crate::config;
use crate::distiller;
use crate::rewrite;

pub async fn run(slug: Option<&str>, all: bool, apply: bool) -> Result<()> {
    if slug.is_none() && !all {
        anyhow::bail!("rewrite: pass a slug, or `--all` to rewrite every page");
    }
    if slug.is_some() && all {
        anyhow::bail!("rewrite: pass a slug OR --all, not both");
    }

    let cfg = config::load(&paths::config_file()?)
        .context("loading config; run `alluvium init` first if this is a fresh install")?;
    let alluvium_root = cfg.default.vault_path.join(&cfg.default.alluvium_subdir);
    if !alluvium_root.join("wiki").exists() {
        println!(
            "rewrite: no wiki under {} yet — nothing to do.",
            alluvium_root.display()
        );
        return Ok(());
    }

    let backend =
        distiller::selection::pick(cfg.default.backend.as_deref(), cfg.default.model.as_deref())?;
    let prompts_dir = paths::prompts_dir()?;
    paths::install_bundled_prompts(&prompts_dir)
        .context("ensuring bundled prompt files are present")?;
    let prompt_path = prompts_dir.join("rewrite.toml");

    let mode = if apply { "apply" } else { "dry-run" };
    tracing::info!(mode = %mode, backend = %backend.kind(), "rewrite: starting");

    if let Some(s) = slug {
        let outcome =
            rewrite::rewrite_one(&alluvium_root, s, backend.as_ref(), &prompt_path, apply).await?;
        print_one(s, &outcome, apply);
        return Ok(());
    }

    // --all path
    let report =
        rewrite::rewrite_all(&alluvium_root, backend.as_ref(), &prompt_path, apply).await?;
    println!(
        "rewrite: {} pages processed ({} rewritten / {} skipped / {} no-block).",
        report.rows.len(),
        report.count("rewritten"),
        report.count("skipped"),
        report.count("no-block"),
    );
    if apply {
        println!(
            "         {} pages actually written to disk.",
            report.applied()
        );
    } else {
        println!("         Dry-run — no files were modified. Re-run with --apply to commit.");
    }
    Ok(())
}

fn print_one(slug: &str, outcome: &rewrite::RewriteOutcome, apply: bool) {
    match outcome {
        rewrite::RewriteOutcome::Rewritten { applied } => {
            if *applied {
                println!("rewrite: ✓ rewrote `{slug}`");
            } else if apply {
                println!("rewrite: ✗ failed to apply `{slug}`");
            } else {
                println!("rewrite: would rewrite `{slug}` (dry-run)");
            }
        }
        rewrite::RewriteOutcome::Skipped => {
            println!("rewrite: SKIP `{slug}` (no durable kernel; consider audit move-to-log)");
        }
        rewrite::RewriteOutcome::NoFactBlock => {
            println!("rewrite: `{slug}` has no alluvium:fact block; nothing to rewrite");
        }
    }
}
