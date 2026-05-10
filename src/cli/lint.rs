//! `alluvium lint [--apply]` — find near-duplicate topic pages and
//! optionally merge them.
//!
//! Default is dry-run: scan, score, ask the LLM, print the report.
//! Pass `--apply` to actually rewrite winning pages and delete losers.

use anyhow::{Context, Result};

use crate::cli::paths;
use crate::config;
use crate::distiller;
use crate::lint;

pub async fn run(apply: bool) -> Result<()> {
    let cfg = config::load(&paths::config_file()?)
        .context("loading config; run `alluvium init` first if this is a fresh install")?;

    let alluvium_root = cfg.default.vault_path.join(&cfg.default.alluvium_subdir);

    if !alluvium_root.join("wiki").exists() {
        println!(
            "lint: no wiki under {} yet — nothing to do.",
            alluvium_root.display()
        );
        return Ok(());
    }

    let backend =
        distiller::selection::pick(cfg.default.backend.as_deref(), cfg.default.model.as_deref())?;
    let prompt_path = paths::prompts_dir()?.join("lint.toml");

    let mode = if apply { "apply" } else { "dry-run" };
    tracing::info!(mode = %mode, backend = %backend.kind(), "lint: starting");

    let report = lint::run_lint(&alluvium_root, backend.as_ref(), &prompt_path, apply).await?;

    if report.rows.is_empty() {
        println!("lint: scanned, no near-duplicate pairs found above threshold.");
        return Ok(());
    }

    println!(
        "lint: {} candidate pair(s) sent to LLM ({} merge / {} keep).",
        report.rows.len(),
        report.merge_decided_count(),
        report.kept_count(),
    );
    if apply {
        println!(
            "      {} merge(s) actually applied to disk.",
            report.merged_count()
        );
    } else {
        println!("      Dry-run — no files were modified. Re-run with --apply to commit.");
    }
    println!();

    for row in &report.rows {
        let glyph = match (row.decision.decision.as_str(), row.applied) {
            ("merge", true) => "✓ merged",
            ("merge", false) if apply => "✗ merge failed",
            ("merge", false) => "→ would merge (dry-run)",
            ("keep", _) => "·  keep separate",
            _ => "?  unknown",
        };
        println!(
            "  {glyph}  [{:.2}]  `{}`  ↔  `{}`",
            row.pair.score, row.pair.slug_a, row.pair.slug_b
        );
        if let Some(reason) = row.decision.reason.as_deref() {
            println!("           reason: {reason}");
        }
        if row.decision.decision == "merge" {
            if let Some(winner) = row.decision.winning_slug.as_deref() {
                println!("           winner: `{winner}`");
            }
        }
    }

    Ok(())
}
