//! `alluvium audit [--apply]` — classify each topic page and (with
//! `--apply`) move episodes to log.md / delete pure noise.

use anyhow::{Context, Result};

use crate::audit;
use crate::cli::paths;
use crate::config;
use crate::distiller;

pub async fn run(apply: bool) -> Result<()> {
    let cfg = config::load(&paths::config_file()?)
        .context("loading config; run `alluvium init` first if this is a fresh install")?;

    let alluvium_root = cfg.default.vault_path.join(&cfg.default.alluvium_subdir);
    if !alluvium_root.join("wiki").exists() {
        println!(
            "audit: no wiki under {} yet — nothing to do.",
            alluvium_root.display()
        );
        return Ok(());
    }

    let backend =
        distiller::selection::pick(cfg.default.backend.as_deref(), cfg.default.model.as_deref())?;

    let prompts_dir = paths::prompts_dir()?;
    paths::install_bundled_prompts(&prompts_dir)
        .context("ensuring bundled prompt files are present")?;
    let prompt_path = prompts_dir.join("audit.toml");

    let mode = if apply { "apply" } else { "dry-run" };
    tracing::info!(mode = %mode, backend = %backend.kind(), "audit: starting");

    let report = audit::run_audit(&alluvium_root, backend.as_ref(), &prompt_path, apply).await?;

    let keep = report.count_with_verdict("keep");
    let move_to_log = report.count_with_verdict("move-to-log");
    let delete = report.count_with_verdict("delete");

    println!(
        "audit: {} pages classified ({} keep / {} move-to-log / {} delete).",
        report.rows.len(),
        keep,
        move_to_log,
        delete
    );
    if apply {
        println!(
            "       {} pages actually rewritten on disk.",
            report.applied_count()
        );
    } else {
        println!("       Dry-run — no files were modified. Re-run with --apply to commit.");
    }
    println!();

    // Group the report for readability: episodes first (the action items),
    // then deletes, then the (boring) kept pages last.
    let mut moves: Vec<_> = report
        .rows
        .iter()
        .filter(|r| r.decision.verdict == "move-to-log")
        .collect();
    let mut deletes: Vec<_> = report
        .rows
        .iter()
        .filter(|r| r.decision.verdict == "delete")
        .collect();
    moves.sort_by(|a, b| a.slug.cmp(&b.slug));
    deletes.sort_by(|a, b| a.slug.cmp(&b.slug));

    let moves_empty = moves.is_empty();
    let deletes_empty = deletes.is_empty();
    if !moves_empty {
        println!("→ move to log.md ({} page(s)):", moves.len());
        for row in &moves {
            let glyph = if row.applied { "✓" } else { "·" };
            println!("  {glyph} `{}`", row.slug);
            if let Some(reason) = row.decision.reason.as_deref() {
                println!("       reason: {reason}");
            }
            if let Some(line) = row.decision.log_line.as_deref() {
                println!("       log:    {line}");
            }
        }
        println!();
    }
    if !deletes_empty {
        println!("✗ delete ({} page(s)):", deletes.len());
        for row in &deletes {
            let glyph = if row.applied { "✓" } else { "·" };
            println!("  {glyph} `{}`", row.slug);
            if let Some(reason) = row.decision.reason.as_deref() {
                println!("       reason: {reason}");
            }
        }
        println!();
    }
    if moves_empty && deletes_empty {
        println!("Everything classified as durable knowledge. Wiki looks healthy.");
    }

    Ok(())
}
