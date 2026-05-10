//! `alluvium archive` — the Stop-hook entry point + manual replay target.
//!
//! Two invocation modes:
//!
//! - **Hook mode** (no `--session`): reads stdin payload from Claude Code.
//! - **Manual mode** (`--session <id>`): looks up transcript by id under
//!   `~/.claude/projects/`. Used by `alluvium replay`.
//!
//! Pipeline:
//!   1. Resolve session_id + transcript_path + cwd
//!   2. Acquire file lock, self-filter check
//!   3. Read transcript (merging PreCompact snapshots if present)
//!   4. Reconstruct + extract metadata
//!   5. Distill via Anthropic API
//!   6. For each fact: decide target page → merge → atomic write
//!   7. Save raw transcript copy to `raw/sessions/`
//!   8. Append to log.md, update index.md
//!   9. Record archive log entry (success or failure)

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

use crate::cli::paths;
use crate::config;
use crate::distiller::{self, DistillerInput, TokenUsage};
use crate::extraction::ExtractedFact;
use crate::hook::{lock, payload, self_filter, spawn};

/// Set by `ClaudeCliBackend` on the spawned `claude` subprocess. When
/// archive runs and finds this in its env, it skips immediately — this
/// breaks the recursion where claude-cli's nested session would itself
/// trigger the Stop hook.
const ALLUVIUM_DISTILLING_ENV: &str = "ALLUVIUM_DISTILLING";
use crate::log::{self as auditlog, ArchiveLogEntry};
use crate::transcript::{self, ConversationData};
use crate::vault;
use crate::wiki;

pub async fn run(session_id_arg: Option<&str>, detached: bool) -> Result<()> {
    // Recursion guard: when ClaudeCliBackend spawns `claude -p`, the nested
    // session's Stop hook would land here again — we'd archive ourselves
    // archiving, infinitely. The backend sets this env var; we no-op.
    if std::env::var(ALLUVIUM_DISTILLING_ENV).is_ok_and(|v| !v.is_empty()) {
        tracing::info!(
            "archive: ALLUVIUM_DISTILLING set; this session is a nested distill call, skipping"
        );
        return Ok(());
    }

    // `--detached` mode (Stop hook): read stdin payload, fork a detached
    // worker that re-enters this command in manual mode (`--session <id>`),
    // return immediately so Claude Code's hook is unblocked. The worker
    // does the real distillation work in the background.
    if detached {
        let p = payload::read_from_stdin().context("reading hook stdin payload")?;
        spawn::spawn_archive_detached(&p.session_id, &p.cwd, None)
            .context("spawning detached archive worker")?;
        return Ok(());
    }

    let started_at = Utc::now();

    let resolved = resolve_session(session_id_arg).await?;
    if let Some(reason) = &resolved.skip_reason {
        tracing::info!(
            session = %resolved.session_id,
            reason = %reason,
            "archive: session skipped"
        );
        return Ok(());
    }

    let result = do_archive(&resolved, started_at).await;
    record_log(&resolved.session_id, started_at, &result);
    result.map(|_| ())
}

#[derive(Debug, Clone)]
struct ResolvedRun {
    session_id: String,
    transcript_path: PathBuf,
    /// Working directory at session start. Currently used only by the
    /// self_filter check; retained for future logging / metadata.
    #[allow(dead_code)]
    cwd: PathBuf,
    skip_reason: Option<String>,
    config: config::ConfigFile,
}

async fn resolve_session(session_id_arg: Option<&str>) -> Result<ResolvedRun> {
    let cfg = config::load(&paths::config_file()?)
        .context("loading config; run `alluvium init` first if this is a fresh install")?;

    let (session_id, transcript_path, cwd) = match session_id_arg {
        Some(id) => {
            let transcript = find_transcript_by_session_id(id)?;
            let cwd = std::env::current_dir().unwrap_or_default();
            (id.to_string(), transcript, cwd)
        }
        None => {
            let p = payload::read_from_stdin().context("reading hook stdin payload")?;
            (p.session_id, p.transcript_path, p.cwd)
        }
    };

    // Canonicalize both sides before the self-filter check. macOS aliases
    // /tmp → /private/tmp and /var → /private/var, so a user's `skip_paths`
    // entry of `/var/folders/.../alluvium` would textually fail to match
    // a `cwd` that came back as `/private/var/folders/.../alluvium`.
    // self_filter::should_skip is documented as "expects canonicalized
    // input"; resolving here keeps that contract honest. Falls back to
    // the original path if canonicalize fails (e.g. dir doesn't exist).
    let cwd_canon = std::fs::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
    let skips_canon: Vec<PathBuf> = cfg
        .default
        .skip_paths
        .iter()
        .map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.clone()))
        .collect();
    let skip_reason = self_filter::should_skip(&cwd_canon, &skips_canon);

    Ok(ResolvedRun {
        session_id,
        transcript_path,
        cwd,
        skip_reason,
        config: cfg,
    })
}

#[derive(Debug)]
struct ArchiveOutcome {
    touched_pages: Vec<PathBuf>,
    usage: Option<TokenUsage>,
}

async fn do_archive(resolved: &ResolvedRun, started_at: DateTime<Utc>) -> Result<ArchiveOutcome> {
    // Lock to serialize with concurrent archive workers.
    let _lock = lock::acquire(&paths::lock_path()?).context("acquiring archive lock")?;

    // Read transcript, merging PreCompact snapshots if any.
    let snapshots_dir = paths::session_snapshots_dir(&resolved.session_id)?;
    let events = if snapshots_dir.exists() {
        transcript::merge_snapshots::merge(&resolved.transcript_path, &snapshots_dir)
            .context("merging transcript with PreCompact snapshots")?
    } else {
        transcript::jsonl::read(&resolved.transcript_path)?
    };

    if events.is_empty() {
        anyhow::bail!("transcript has no events; nothing to archive");
    }

    let metadata = transcript::metadata::extract(&events)?;
    let messages = transcript::reconstruct::run(&events);
    let conversation = ConversationData {
        session_id: resolved.session_id.clone(),
        messages,
        metadata,
    };

    // Distill.
    let prompts_dir = paths::prompts_dir()?;
    let template = distiller::prompt::load(&resolved.config.default.recipe, &prompts_dir)
        .context("loading prompt template")?;
    let distiller_input = DistillerInput {
        conversation: conversation.clone(),
        recipe_name: resolved.config.default.recipe.clone(),
    };
    let prompt = distiller::prompt::render(&template, &distiller_input)?;
    let backend = distiller::selection::pick(
        resolved.config.default.backend.as_deref(),
        resolved.config.default.model.as_deref(),
    )?;
    tracing::info!(backend = %backend.kind(), "archive: distilling");

    // --debug: dump distiller input + rendered prompt before the LLM call.
    // We write before/after each stage so partial failures still leave
    // breadcrumbs (e.g. if the LLM call hangs, input.json is already there).
    if crate::debug_enabled() {
        dump_debug(
            &resolved.session_id,
            "distiller_input.json",
            &distiller_input,
        );
        dump_debug(&resolved.session_id, "rendered_prompt.json", &prompt);
    }

    let response = backend
        .complete(&prompt)
        .await
        .context("calling LLM backend")?;

    if crate::debug_enabled() {
        dump_debug_text(&resolved.session_id, "llm_raw_output.txt", &response.text);
    }

    let output = distiller::parser::parse(&response.text, response.usage)?;

    if crate::debug_enabled() {
        dump_debug(&resolved.session_id, "distiller_output.json", &output);
    }

    let alluvium_root = resolved
        .config
        .default
        .vault_path
        .join(&resolved.config.default.alluvium_subdir);

    // Save raw transcript copy.
    let raw_basename = format!(
        "{}_{}",
        started_at.format("%Y-%m-%dT%H-%M"),
        short_session(&resolved.session_id)
    );
    let raw_filename = format!("{raw_basename}.md");
    let raw_path = alluvium_root
        .join("raw")
        .join("sessions")
        .join(&raw_filename);
    save_raw_transcript(&raw_path, &resolved.transcript_path, &output, started_at)
        .context("saving raw transcript copy")?;
    let source_link = format!("[[../raw/sessions/{raw_basename}]]");

    // Write each fact's topic page.
    let mut touched = Vec::new();
    for fact in &output.facts {
        match write_fact(&alluvium_root, fact, &source_link, started_at) {
            Ok(path) => touched.push(path),
            Err(err) => {
                tracing::warn!(
                    slug = %fact.page_slug,
                    error = %format!("{err:#}"),
                    "failed to write fact; continuing with others"
                );
            }
        }
    }

    // Append to log.md (paths relative to alluvium_root, as wikilink-friendly slugs).
    let log_md_path = alluvium_root.join("wiki").join("log.md");
    let log_links: Vec<String> = touched
        .iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(&alluvium_root).ok()?;
            let no_wiki = rel.strip_prefix("wiki").unwrap_or(rel);
            let no_ext = no_wiki.with_extension("");
            Some(no_ext.to_string_lossy().into_owned())
        })
        .collect();
    if let Err(err) =
        vault::log_appender::append(&log_md_path, &started_at, &output.title, &log_links)
    {
        tracing::warn!(error = %format!("{err:#}"), "log.md append failed");
    }

    // Update index.md.
    if let Err(err) = vault::index_updater::update(&alluvium_root) {
        tracing::warn!(error = %format!("{err:#}"), "index.md update failed");
    }

    Ok(ArchiveOutcome {
        touched_pages: touched
            .iter()
            .filter_map(|p| p.strip_prefix(&alluvium_root).ok())
            .map(PathBuf::from)
            .collect(),
        usage: output.usage,
    })
}

fn write_fact(
    alluvium_root: &Path,
    fact: &ExtractedFact,
    source_link: &str,
    now: DateTime<Utc>,
) -> Result<PathBuf> {
    let target = wiki::decide::decide(alluvium_root, fact);
    let content = match &target {
        wiki::TargetPage::New(_) => vault::merger::render_new_page(fact, source_link, now)?,
        wiki::TargetPage::Existing(path) => {
            let existing = std::fs::read_to_string(path)
                .with_context(|| format!("reading existing topic page {}", path.display()))?;
            vault::merger::merge_into_existing(&existing, fact, source_link, now)?
        }
    };
    vault::writer::write_atomic(target.path(), &content)?;
    Ok(target.path().to_path_buf())
}

fn save_raw_transcript(
    path: &Path,
    transcript_path: &Path,
    output: &distiller::DistillerOutput,
    now: DateTime<Utc>,
) -> Result<()> {
    let content = std::fs::read_to_string(transcript_path).unwrap_or_default();
    let tags_yaml: String = if output.tags.is_empty() {
        "tags: []".into()
    } else {
        let lines: Vec<String> = output.tags.iter().map(|t| format!("  - {t}")).collect();
        format!("tags:\n{}", lines.join("\n"))
    };
    let body = format!(
        "---\ntitle: {title}\ntype: source\ncreated: {created}\n{tags}\n---\n\n# {title}\n\nRaw Claude Code session transcript (JSONL).\n\n```jsonl\n{content}\n```\n",
        title = output.title,
        created = now.format("%Y-%m-%d"),
        tags = tags_yaml,
        content = content,
    );
    vault::writer::write_atomic(path, &body)
}

fn record_log(session_id: &str, started_at: DateTime<Utc>, result: &Result<ArchiveOutcome>) {
    let finished_at = Utc::now();
    let entry = match result {
        Ok(out) => ArchiveLogEntry {
            session_id: session_id.to_string(),
            started_at,
            finished_at,
            touched_pages: out.touched_pages.clone(),
            error: None,
            input_tokens: out.usage.map(|u| u.input_tokens),
            output_tokens: out.usage.map(|u| u.output_tokens),
        },
        Err(err) => ArchiveLogEntry {
            session_id: session_id.to_string(),
            started_at,
            finished_at,
            touched_pages: vec![],
            error: Some(format!("{err:#}")),
            input_tokens: None,
            output_tokens: None,
        },
    };
    if let Ok(p) = paths::archive_log_path() {
        let _ = auditlog::record(&p, &entry);
    }
}

fn find_transcript_by_session_id(session_id: &str) -> Result<PathBuf> {
    let projects = home_dir()
        .context("no home directory")?
        .join(".claude")
        .join("projects");
    if !projects.exists() {
        anyhow::bail!(
            "Claude Code transcripts directory does not exist: {}",
            projects.display()
        );
    }
    for project_dir in std::fs::read_dir(&projects)?.flatten() {
        let candidate = project_dir.path().join(format!("{session_id}.jsonl"));
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    anyhow::bail!("no transcript found for session id {session_id}")
}

/// First 12 chars of the session id — enough to keep distinct UUID
/// sessions distinct in `raw/sessions/<ts>_<short>.md` filenames while
/// staying short enough to read at a glance. (UUID v4 first 8 chars are
/// fully random, so 12 keeps collision probability negligible across a
/// realistic vault. 8 used to be enough but collided on hand-crafted
/// session ids in tests; 12 is the smallest bump that fixes that.)
fn short_session(id: &str) -> String {
    id.chars().take(12).collect()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Write `value` (serialized as pretty JSON) into the debug dir under
/// `<filename>`. Failures are logged at warn level but never propagated —
/// debugging artifacts are best-effort and must not break archival.
fn dump_debug<T: serde::Serialize>(session_id: &str, filename: &str, value: &T) {
    let Ok(dir) = paths::debug_dir(session_id) else {
        return;
    };
    if let Err(err) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %err, dir = %dir.display(), "debug dump: mkdir failed");
        return;
    }
    match serde_json::to_string_pretty(value) {
        Ok(s) => {
            let path = dir.join(filename);
            if let Err(err) = std::fs::write(&path, s) {
                tracing::warn!(error = %err, path = %path.display(), "debug dump: write failed");
            } else {
                tracing::info!(path = %path.display(), "debug dump: wrote");
            }
        }
        Err(err) => {
            tracing::warn!(error = %err, "debug dump: JSON serialize failed");
        }
    }
}

/// Like `dump_debug` but for raw text (LLM output before parsing).
fn dump_debug_text(session_id: &str, filename: &str, text: &str) {
    let Ok(dir) = paths::debug_dir(session_id) else {
        return;
    };
    if let Err(err) = std::fs::create_dir_all(&dir) {
        tracing::warn!(error = %err, dir = %dir.display(), "debug dump: mkdir failed");
        return;
    }
    let path = dir.join(filename);
    if let Err(err) = std::fs::write(&path, text) {
        tracing::warn!(error = %err, path = %path.display(), "debug dump: write failed");
    } else {
        tracing::info!(path = %path.display(), "debug dump: wrote");
    }
}
