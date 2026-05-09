//! SessionStart hook handler.
//!
//! Reads the stdin payload, loads config, computes the self-filter decision,
//! and writes a `resolved.json` to `<cache>/sessions/<id>/` so subsequent
//! hooks can short-circuit on `skip_reason` without re-resolving.
//!
//! Must return within 50 ms (per docs/HOOKS.md).

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cli::paths;
use crate::config;
use crate::hook::{payload, self_filter};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSession {
    pub session_id: String,
    pub started_at: String,
    pub cwd: std::path::PathBuf,
    pub transcript_path: std::path::PathBuf,
    pub skip_reason: Option<String>,
    pub vault_path: std::path::PathBuf,
    pub alluvium_subdir: String,
    pub recipe: String,
    pub model: String,
    pub keep_source_summaries: bool,
}

pub async fn run() -> Result<()> {
    let p = payload::read_from_stdin()?;
    let config_path = paths::config_file()?;
    // If config doesn't exist, the user hasn't run `alluvium init`. Don't
    // fail the hook (Claude Code shouldn't be impacted) — just record and
    // mark the session as skipped.
    let cfg = match config::load(&config_path) {
        Ok(c) => c,
        Err(_) => {
            tracing::warn!(
                "no config found at {}; skipping session (run `alluvium init` to enable archiving)",
                config_path.display()
            );
            write_resolved_skipped(&p, "config-missing")?;
            return Ok(());
        }
    };

    let skip_reason = self_filter::should_skip(&p.cwd, &cfg.default.skip_paths);

    let resolved = ResolvedSession {
        session_id: p.session_id.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        cwd: p.cwd.clone(),
        transcript_path: p.transcript_path.clone(),
        skip_reason,
        vault_path: cfg.default.vault_path,
        alluvium_subdir: cfg.default.alluvium_subdir,
        recipe: cfg.default.recipe,
        model: cfg.default.model,
        keep_source_summaries: cfg.default.keep_source_summaries,
    };

    write_resolved(&p.session_id, &resolved)?;
    Ok(())
}

fn write_resolved(session_id: &str, resolved: &ResolvedSession) -> Result<()> {
    let path = paths::session_resolved_json(session_id)?;
    let json = serde_json::to_string_pretty(resolved)?;
    crate::vault::writer::write_atomic(&path, &json)
}

fn write_resolved_skipped(p: &payload::HookPayload, reason: &str) -> Result<()> {
    let resolved = ResolvedSession {
        session_id: p.session_id.clone(),
        started_at: chrono::Utc::now().to_rfc3339(),
        cwd: p.cwd.clone(),
        transcript_path: p.transcript_path.clone(),
        skip_reason: Some(reason.into()),
        vault_path: std::path::PathBuf::new(),
        alluvium_subdir: String::new(),
        recipe: String::new(),
        model: String::new(),
        keep_source_summaries: false,
    };
    write_resolved(&p.session_id, &resolved)
}
