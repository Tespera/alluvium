//! `alluvium audit` — classify each existing topic page as durable
//! knowledge / episode / noise, and (optionally) clean the vault.
//!
//! Companion to the distill-stage episode filter. Distill prevents new
//! episode-class facts from being written; audit removes pre-existing
//! ones that landed before the filter existed, plus the occasional one
//! that slips through.
//!
//! Three verdicts per page:
//!
//! - **keep** — page is durable; leave alone.
//! - **move-to-log** — page is episodic; append a one-line summary to
//!   `wiki/log.md` (preserves the historical fact in a place that's
//!   *allowed* to hold episodes) and remove the page from the wiki.
//! - **delete** — page is noise; remove without preserving a log line.
//!
//! Dry-run prints decisions only. `--apply` actually rewrites disk.

use anyhow::{Context, Result};
use chrono::Utc;
use minijinja::{context, Environment};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::distiller::backend::{LlmBackend, RenderedPrompt};
use crate::vault::frontmatter;
use crate::wiki::index_scan;

#[derive(Debug, Clone, Deserialize)]
pub struct AuditDecision {
    pub verdict: String, // "keep" | "move-to-log" | "delete"
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub log_line: Option<String>,
}

#[derive(Debug)]
pub struct AuditReportRow {
    pub path: PathBuf,
    pub slug: String,
    pub decision: AuditDecision,
    pub applied: bool,
}

#[derive(Debug)]
pub struct AuditReport {
    pub rows: Vec<AuditReportRow>,
}

impl AuditReport {
    pub fn count_with_verdict(&self, v: &str) -> usize {
        self.rows.iter().filter(|r| r.decision.verdict == v).count()
    }
    pub fn applied_count(&self) -> usize {
        self.rows.iter().filter(|r| r.applied).count()
    }
}

#[derive(Debug, Deserialize)]
struct AuditFile {
    meta: AuditMeta,
    prompt: AuditPrompt,
}

#[derive(Debug, Deserialize)]
struct AuditMeta {
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default = "default_max_tokens")]
    max_tokens: u32,
}

fn default_max_tokens() -> u32 {
    1024
}

#[derive(Debug, Deserialize)]
struct AuditPrompt {
    system: String,
    user_template: String,
}

/// Walk `wiki/{concepts,entities}/*.md`, ask the LLM for a verdict on
/// each. With `apply=true` the verdict is actually executed (move-to-log
/// appends a line and deletes the page; delete just removes the file).
/// On dry-run, the report shows decisions but nothing is written.
pub async fn run_audit(
    alluvium_root: &Path,
    backend: &dyn LlmBackend,
    prompt_template_path: &Path,
    apply: bool,
) -> Result<AuditReport> {
    let topics = index_scan::scan(alluvium_root)?;
    let mut rows = Vec::with_capacity(topics.len());

    for topic in topics {
        let path = page_path(alluvium_root, &topic);
        let decision = match classify_page(&path, &topic, backend, prompt_template_path).await {
            Ok(d) => d,
            Err(err) => {
                tracing::warn!(
                    slug = %topic.slug,
                    error = %format!("{err:#}"),
                    "audit: LLM classification failed; defaulting to keep"
                );
                AuditDecision {
                    verdict: "keep".into(),
                    reason: Some(format!("LLM error: {err}")),
                    log_line: None,
                }
            }
        };

        let applied = if apply {
            match apply_decision(alluvium_root, &path, &topic.slug, &decision) {
                Ok(did_something) => did_something,
                Err(err) => {
                    tracing::error!(
                        slug = %topic.slug,
                        error = %format!("{err:#}"),
                        "audit: apply failed; page left untouched"
                    );
                    false
                }
            }
        } else {
            false
        };

        rows.push(AuditReportRow {
            path,
            slug: topic.slug,
            decision,
            applied,
        });
    }

    // After apply, the index is stale (we may have deleted N pages).
    // Regenerate; failure is non-fatal — the next archive will heal it.
    let any_removed = rows
        .iter()
        .any(|r| r.applied && r.decision.verdict != "keep");
    if apply && any_removed {
        if let Err(err) = crate::vault::index_updater::update(alluvium_root) {
            tracing::warn!(
                error = %format!("{err:#}"),
                "audit: post-apply index regen failed; will self-heal on next archive"
            );
        }
    }

    Ok(AuditReport { rows })
}

fn page_path(alluvium_root: &Path, t: &index_scan::TopicEntry) -> PathBuf {
    alluvium_root
        .join("wiki")
        .join(&t.kind)
        .join(format!("{}.md", t.slug))
}

async fn classify_page(
    path: &Path,
    topic: &index_scan::TopicEntry,
    backend: &dyn LlmBackend,
    prompt_template_path: &Path,
) -> Result<AuditDecision> {
    let raw = std::fs::read_to_string(prompt_template_path).with_context(|| {
        format!(
            "reading audit prompt template {}",
            prompt_template_path.display()
        )
    })?;
    let parsed: AuditFile = toml::from_str(&raw).with_context(|| {
        format!(
            "parsing audit prompt template {}",
            prompt_template_path.display()
        )
    })?;

    let page_raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading page {}", path.display()))?;
    let (_, body) = frontmatter::parse(&page_raw)?;

    let mut env = Environment::new();
    env.add_template("audit_user", &parsed.prompt.user_template)
        .context("compiling audit user_template")?;
    let template = env.get_template("audit_user").unwrap();
    let user = template
        .render(context! {
            slug => topic.slug,
            title => topic.title,
            page_type => topic.kind,
            body => body,
        })
        .context("rendering audit user_template")?;

    let prompt = RenderedPrompt {
        system: parsed.prompt.system,
        user,
        model: parsed.meta.model,
        max_tokens: parsed.meta.max_tokens,
    };

    let response = backend
        .complete(&prompt)
        .await
        .context("calling LLM for audit")?;
    parse_decision(&response.text)
}

fn parse_decision(text: &str) -> Result<AuditDecision> {
    let unfenced = unwrap_code_fence(text);
    let json_str =
        locate_json_object(unfenced).context("could not locate JSON object in audit output")?;
    let decision: AuditDecision = serde_json::from_str(json_str).with_context(|| {
        let preview: String = json_str.chars().take(500).collect();
        format!("audit output was not valid JSON; first 500 chars: {preview}")
    })?;
    match decision.verdict.as_str() {
        "keep" | "move-to-log" | "delete" => Ok(decision),
        other => anyhow::bail!("unknown audit verdict {other:?}"),
    }
}

fn unwrap_code_fence(s: &str) -> &str {
    let trimmed = s.trim();
    let inner = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|rest| rest.trim_start_matches('\n'))
        .map(str::trim_start);
    let Some(after_open) = inner else {
        return trimmed;
    };
    after_open
        .strip_suffix("```")
        .map(str::trim)
        .unwrap_or(after_open)
}

fn locate_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end > start {
        Some(&s[start..=end])
    } else {
        None
    }
}

/// Execute the verdict. Returns `Ok(true)` when something on disk
/// changed, `Ok(false)` for keep (no-op).
fn apply_decision(
    alluvium_root: &Path,
    path: &Path,
    slug: &str,
    decision: &AuditDecision,
) -> Result<bool> {
    match decision.verdict.as_str() {
        "keep" => Ok(false),
        "move-to-log" => {
            let line = decision.log_line.as_deref().unwrap_or("(no summary)");
            let log_path = alluvium_root.join("wiki").join("log.md");
            let entry = format!(
                "\n## [{date}] audit | moved `{slug}` → log\n  {line}\n",
                date = Utc::now().format("%Y-%m-%d"),
            );
            append_log(&log_path, &entry)?;
            std::fs::remove_file(path)
                .with_context(|| format!("removing audited page {}", path.display()))?;
            Ok(true)
        }
        "delete" => {
            std::fs::remove_file(path)
                .with_context(|| format!("removing audited page {}", path.display()))?;
            // Also log the deletion so a curious future-user can see
            // that audit acted on this slug.
            let log_path = alluvium_root.join("wiki").join("log.md");
            let entry = format!(
                "\n## [{date}] audit | deleted `{slug}`\n  Reason: {reason}\n",
                date = Utc::now().format("%Y-%m-%d"),
                reason = decision.reason.as_deref().unwrap_or("(no reason)"),
            );
            append_log(&log_path, &entry)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn append_log(path: &Path, entry: &str) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {} for append", path.display()))?;
    f.write_all(entry.as_bytes())
        .with_context(|| format!("appending to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_keep() {
        let json = r#"{"verdict":"keep","reason":"timeless concept","log_line":null}"#;
        let d = parse_decision(json).unwrap();
        assert_eq!(d.verdict, "keep");
    }

    #[test]
    fn parse_move_to_log() {
        let json = r#"{"verdict":"move-to-log","reason":"event report","log_line":"PR #90 was reverted on 2026-05-09"}"#;
        let d = parse_decision(json).unwrap();
        assert_eq!(d.verdict, "move-to-log");
        assert!(d.log_line.is_some());
    }

    #[test]
    fn parse_delete() {
        let json = r#"{"verdict":"delete","reason":"verbatim chat noise"}"#;
        let d = parse_decision(json).unwrap();
        assert_eq!(d.verdict, "delete");
    }

    #[test]
    fn parse_unwraps_code_fence() {
        let json = "```json\n{\"verdict\":\"keep\"}\n```";
        let d = parse_decision(json).unwrap();
        assert_eq!(d.verdict, "keep");
    }

    #[test]
    fn parse_rejects_unknown_verdict() {
        let json = r#"{"verdict":"maybe"}"#;
        assert!(parse_decision(json).is_err());
    }
}
