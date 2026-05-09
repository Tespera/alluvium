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
use crate::hook::{lock, payload, self_filter};
use crate::log::{self as auditlog, ArchiveLogEntry};
use crate::transcript::{self, ConversationData};
use crate::vault;
use crate::wiki;

pub async fn run(session_id_arg: Option<&str>) -> Result<()> {
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

    let skip_reason = self_filter::should_skip(&cwd, &cfg.default.skip_paths);

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
    let api_key = config::secrets::get_api_key()?;
    let prompts_dir = paths::prompts_dir()?;
    let template = distiller::prompt::load(&resolved.config.default.recipe, &prompts_dir)
        .context("loading prompt template")?;
    let request = distiller::prompt::render(
        &template,
        &DistillerInput {
            conversation: conversation.clone(),
            recipe_name: resolved.config.default.recipe.clone(),
        },
    )?;
    let client = distiller::client::AnthropicClient::new(api_key);
    let response = client
        .messages(request)
        .await
        .context("calling Anthropic API")?;
    let output = distiller::parser::parse(&response)?;

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

fn short_session(id: &str) -> String {
    id.chars().take(8).collect()
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
