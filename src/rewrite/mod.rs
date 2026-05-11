//! `alluvium rewrite` — reframe episodic-prose topic pages as timeless.
//!
//! Companion to `audit` (decides "this page is episodic") and the
//! distill stage's episode filter (prevents new episodes). Rewrite is
//! the *salvage* path: a page got into the wiki with valuable kernel
//! buried in "today we / the user / PR #N" framing. Rewrite asks the
//! LLM to keep the kernel and drop the scaffolding.
//!
//! Operates on the body inside the first `alluvium:fact` block (the
//! Alluvium-owned region). Frontmatter and content outside markers
//! are preserved.

use anyhow::{Context, Result};
use chrono::Utc;
use minijinja::{context, Environment};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::distiller::backend::{LlmBackend, RenderedPrompt};
use crate::vault::{frontmatter, merger, writer};
use crate::wiki::index_scan;

const FACT_OPEN_PREFIX: &str = "<!-- alluvium:fact id=";
const FACT_OPEN_SUFFIX: &str = " -->";
const FACT_CLOSE: &str = "<!-- alluvium:end -->";

#[derive(Debug, Deserialize)]
struct RewriteFile {
    meta: RewriteMeta,
    prompt: RewritePrompt,
}

#[derive(Debug, Deserialize)]
struct RewriteMeta {
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
    3072
}

#[derive(Debug, Deserialize)]
struct RewritePrompt {
    system: String,
    user_template: String,
}

#[derive(Debug)]
pub enum RewriteOutcome {
    /// Rewrite produced a new body. The page on disk was updated when
    /// `apply=true`, left alone (preview-only) otherwise.
    Rewritten { applied: bool },
    /// The LLM voted SKIP — no durable kernel to salvage. Page left
    /// untouched; user may want to audit this page separately for
    /// move-to-log.
    Skipped,
    /// The page has no `alluvium:fact` block, so rewrite has nothing
    /// to operate on. Left untouched.
    NoFactBlock,
}

#[derive(Debug)]
pub struct RewriteReport {
    pub rows: Vec<(String, RewriteOutcome)>,
}

impl RewriteReport {
    pub fn count(&self, want: &str) -> usize {
        self.rows
            .iter()
            .filter(|(_, o)| {
                matches!(
                    (o, want),
                    (RewriteOutcome::Rewritten { .. }, "rewritten")
                        | (RewriteOutcome::Skipped, "skipped")
                        | (RewriteOutcome::NoFactBlock, "no-block")
                )
            })
            .count()
    }
    pub fn applied(&self) -> usize {
        self.rows
            .iter()
            .filter(|(_, o)| matches!(o, RewriteOutcome::Rewritten { applied: true }))
            .count()
    }
}

/// Rewrite one specific topic page by slug. Used by the single-page
/// CLI path (`alluvium rewrite <slug>`).
pub async fn rewrite_one(
    alluvium_root: &Path,
    slug: &str,
    backend: &dyn LlmBackend,
    prompt_template_path: &Path,
    apply: bool,
) -> Result<RewriteOutcome> {
    let path = find_page(alluvium_root, slug).with_context(|| {
        format!(
            "no topic page for slug {slug:?} under {}/wiki/",
            alluvium_root.display()
        )
    })?;
    rewrite_path(&path, slug, backend, prompt_template_path, apply).await
}

/// Walk every concept/entity page and rewrite each. Used by
/// `alluvium rewrite --all`. Errors per page are logged at warn level
/// and the iteration continues — no single bad page derails the batch.
pub async fn rewrite_all(
    alluvium_root: &Path,
    backend: &dyn LlmBackend,
    prompt_template_path: &Path,
    apply: bool,
) -> Result<RewriteReport> {
    let topics = index_scan::scan(alluvium_root)?;
    let mut rows = Vec::with_capacity(topics.len());

    for topic in topics {
        let path = alluvium_root
            .join("wiki")
            .join(&topic.kind)
            .join(format!("{}.md", topic.slug));
        let outcome =
            match rewrite_path(&path, &topic.slug, backend, prompt_template_path, apply).await {
                Ok(o) => o,
                Err(err) => {
                    tracing::warn!(
                        slug = %topic.slug,
                        error = %format!("{err:#}"),
                        "rewrite: page failed; leaving untouched"
                    );
                    RewriteOutcome::Skipped
                }
            };
        rows.push((topic.slug, outcome));
    }

    Ok(RewriteReport { rows })
}

fn find_page(alluvium_root: &Path, slug: &str) -> Option<PathBuf> {
    for sub in ["concepts", "entities"] {
        let p = alluvium_root
            .join("wiki")
            .join(sub)
            .join(format!("{slug}.md"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

async fn rewrite_path(
    path: &Path,
    slug: &str,
    backend: &dyn LlmBackend,
    prompt_template_path: &Path,
    apply: bool,
) -> Result<RewriteOutcome> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let (mut fm, body) = frontmatter::parse(&raw)?;

    // Find the first (and usually only) alluvium:fact block; that's
    // the Alluvium-owned region we may rewrite. If there's no block,
    // there's nothing here we own — leave the page alone.
    let block = match find_first_block(&body) {
        Some(b) => b,
        None => return Ok(RewriteOutcome::NoFactBlock),
    };

    let title = fm
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or(slug)
        .to_string();

    let prompt = render_prompt(prompt_template_path, slug, &title, &block.body)?;
    let response = backend
        .complete(&prompt)
        .await
        .context("calling LLM for rewrite")?;
    let rewritten = strip_wrapper(&response.text);

    if rewritten.trim() == "SKIP" || rewritten.trim().is_empty() {
        return Ok(RewriteOutcome::Skipped);
    }

    if !apply {
        return Ok(RewriteOutcome::Rewritten { applied: false });
    }

    // Build the replacement block (new fact_id since content changed).
    let synthetic = format!("rewritten@{}", Utc::now().format("%Y-%m-%d"));
    let new_id = merger::fact_id(slug, &synthetic);
    let new_block = format!(
        "{open}{id}{close_marker}\n{body}\n{end}",
        open = FACT_OPEN_PREFIX,
        id = new_id,
        close_marker = FACT_OPEN_SUFFIX,
        body = rewritten.trim(),
        end = FACT_CLOSE,
    );

    // Splice the new block into the body in the same position.
    let mut new_body = String::with_capacity(body.len() + new_block.len());
    new_body.push_str(&body[..block.start]);
    new_body.push_str(&new_block);
    new_body.push_str(&body[block.end..]);

    // Bump `updated:` in frontmatter so Obsidian shows a fresh mtime.
    if let Some(map) = fm.as_mapping_mut() {
        map.insert(
            serde_yaml::Value::String("updated".into()),
            serde_yaml::Value::String(Utc::now().format("%Y-%m-%d").to_string()),
        );
    }

    let new_content = frontmatter::render(&fm, &new_body)?;
    writer::write_atomic(path, &new_content)?;
    Ok(RewriteOutcome::Rewritten { applied: true })
}

#[derive(Debug)]
struct Block {
    start: usize,
    end: usize,
    body: String,
}

fn find_first_block(body: &str) -> Option<Block> {
    let open = body.find(FACT_OPEN_PREFIX)?;
    let after_prefix = open + FACT_OPEN_PREFIX.len();
    let suffix_rel = body[after_prefix..].find(FACT_OPEN_SUFFIX)?;
    let body_start = after_prefix + suffix_rel + FACT_OPEN_SUFFIX.len();
    let close_rel = body[body_start..].find(FACT_CLOSE)?;
    let body_end = body_start + close_rel;
    let block_end = body_end + FACT_CLOSE.len();
    Some(Block {
        start: open,
        end: block_end,
        body: body[body_start..body_end].trim().to_string(),
    })
}

fn render_prompt(
    template_path: &Path,
    slug: &str,
    title: &str,
    body: &str,
) -> Result<RenderedPrompt> {
    let raw = std::fs::read_to_string(template_path).with_context(|| {
        format!(
            "reading rewrite prompt template {}",
            template_path.display()
        )
    })?;
    let parsed: RewriteFile = toml::from_str(&raw).with_context(|| {
        format!(
            "parsing rewrite prompt template {}",
            template_path.display()
        )
    })?;
    let mut env = Environment::new();
    env.add_template("rewrite_user", &parsed.prompt.user_template)
        .context("compiling rewrite user_template")?;
    let template = env.get_template("rewrite_user").unwrap();
    let user = template
        .render(context! { slug => slug, title => title, body => body })
        .context("rendering rewrite user_template")?;

    Ok(RenderedPrompt {
        system: parsed.prompt.system,
        user,
        model: parsed.meta.model,
        max_tokens: parsed.meta.max_tokens,
    })
}

fn strip_wrapper(text: &str) -> String {
    let trimmed = text.trim();
    let inner = trimmed
        .strip_prefix("```markdown")
        .or_else(|| trimmed.strip_prefix("```md"))
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim_start_matches('\n'))
        .map(|s| s.trim_start());
    let after_open = match inner {
        Some(s) => s,
        None => return trimmed.to_string(),
    };
    after_open
        .strip_suffix("```")
        .map(str::trim)
        .unwrap_or(after_open)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_first_block_returns_inner() {
        let body =
            "intro\n<!-- alluvium:fact id=aaaa1111 -->\nbody content\n<!-- alluvium:end -->\nafter";
        let b = find_first_block(body).unwrap();
        assert_eq!(b.body, "body content");
    }

    #[test]
    fn find_first_block_returns_none_for_unmarked_page() {
        assert!(find_first_block("plain text, no markers").is_none());
    }

    #[test]
    fn strip_wrapper_handles_markdown_fence() {
        assert_eq!(strip_wrapper("```markdown\nbody\n```"), "body");
        assert_eq!(strip_wrapper("```\nbody\n```"), "body");
        assert_eq!(strip_wrapper("body"), "body");
    }

    #[test]
    fn strip_wrapper_passes_skip_through() {
        assert_eq!(strip_wrapper("SKIP"), "SKIP");
        assert_eq!(strip_wrapper(" SKIP "), "SKIP");
    }
}
