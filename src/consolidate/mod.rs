//! Defends against append-only drift in the wiki.
//!
//! v0.1: manual command (`alluvium consolidate <slug>`) takes one topic
//! page that has accumulated several `<!-- alluvium:fact id=... -->` blocks
//! across sessions and asks the LLM to rewrite them into ONE consolidated
//! block. Frontmatter is preserved verbatim. Content *outside* Alluvium's
//! markers AND BEFORE the first / AFTER the last block survives untouched.
//! User content interleaved BETWEEN blocks is collapsed into the
//! consolidated block (documented limitation — users who want to keep
//! interstitial notes should move them above the first block or below
//! the last one before consolidating).
//!
//! v0.2: scheduled (cron / launchd) runs + heuristic auto-selection.

use anyhow::{Context, Result};
use chrono::Utc;
use minijinja::{context, Environment};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::distiller::backend::{LlmBackend, RenderedPrompt};
use crate::vault::{frontmatter, merger, writer};

const FACT_OPEN_PREFIX: &str = "<!-- alluvium:fact id=";
const FACT_OPEN_SUFFIX: &str = " -->";
const FACT_CLOSE: &str = "<!-- alluvium:end -->";

#[derive(Debug, Deserialize)]
struct ConsolidateFile {
    meta: ConsolidateMeta,
    prompt: ConsolidatePrompt,
}

#[derive(Debug, Deserialize)]
struct ConsolidateMeta {
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
    4096
}

#[derive(Debug, Deserialize)]
struct ConsolidatePrompt {
    system: String,
    user_template: String,
}

/// Outcome reported back to the CLI handler so it can print a useful
/// message (or stay quiet on no-op).
#[derive(Debug)]
pub struct ConsolidateOutcome {
    pub touched_path: PathBuf,
    pub fragments_collapsed: usize,
}

/// Locate the topic page for `slug` under `<alluvium_root>/wiki/`. Tries
/// `concepts/` first, then `entities/`. Returns `None` if neither exists.
pub fn find_page(alluvium_root: &Path, slug: &str) -> Option<PathBuf> {
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

/// Run the consolidation: read the page at `path`, collapse all
/// `alluvium:fact` blocks via the LLM, write back atomically. Returns
/// `Ok(None)` (no-op) when the page has fewer than 2 blocks — there's
/// nothing to consolidate.
pub async fn consolidate_page(
    path: &Path,
    backend: &dyn LlmBackend,
    prompt_template_path: &Path,
) -> Result<Option<ConsolidateOutcome>> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading topic page {}", path.display()))?;
    let (fm, body) = frontmatter::parse(&content)?;

    let blocks = extract_blocks(&body);
    if blocks.len() < 2 {
        return Ok(None);
    }

    let page_title = fm
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled")
        .to_string();

    let rendered = render_prompt(prompt_template_path, &page_title, &blocks)?;
    let response = backend
        .complete(&rendered)
        .await
        .context("calling LLM backend for consolidate")?;

    let consolidated_body = strip_wrapper(&response.text);
    if consolidated_body.trim().is_empty() {
        anyhow::bail!("LLM returned empty consolidated body");
    }

    let new_body = replace_blocks_with_single(&body, &blocks, &page_title, &consolidated_body);
    let new_content = frontmatter::render(&fm, &new_body)?;
    writer::write_atomic(path, &new_content)?;

    Ok(Some(ConsolidateOutcome {
        touched_path: path.to_path_buf(),
        fragments_collapsed: blocks.len(),
    }))
}

#[derive(Debug)]
struct Block {
    /// Byte offset of the open marker in `body`.
    start: usize,
    /// Byte offset just past the close marker.
    end: usize,
    /// The marker block's body content (no markers).
    body: String,
}

/// Find every `<!-- alluvium:fact id=... -->...<!-- alluvium:end -->`
/// pair in source order. Tolerates leading/trailing whitespace. A
/// missing close marker drops the orphan block silently — better than
/// failing the whole consolidate, since the file is still recoverable.
fn extract_blocks(body: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while let Some(rel_open) = body[cursor..].find(FACT_OPEN_PREFIX) {
        let open_start = cursor + rel_open;
        let after_prefix = open_start + FACT_OPEN_PREFIX.len();
        let suffix_rel = match body[after_prefix..].find(FACT_OPEN_SUFFIX) {
            Some(i) => i,
            None => break,
        };
        let body_start = after_prefix + suffix_rel + FACT_OPEN_SUFFIX.len();
        let close_rel = match body[body_start..].find(FACT_CLOSE) {
            Some(i) => i,
            None => break,
        };
        let body_end = body_start + close_rel;
        let block_end = body_end + FACT_CLOSE.len();
        let inner = body[body_start..body_end].trim().to_string();
        out.push(Block {
            start: open_start,
            end: block_end,
            body: inner,
        });
        cursor = block_end;
    }
    out
}

/// Build the new body: keep everything before the FIRST block, drop
/// every block plus the slack between them, insert ONE new block
/// holding `consolidated`, then keep everything after the LAST block.
fn replace_blocks_with_single(
    body: &str,
    blocks: &[Block],
    page_title: &str,
    consolidated: &str,
) -> String {
    let first = &blocks[0];
    let last = &blocks[blocks.len() - 1];

    let prefix = &body[..first.start];
    let suffix = &body[last.end..];

    // Synthetic stable id for the consolidated block. Re-running on the
    // same day is idempotent (same id → would replace), running tomorrow
    // produces a fresh id.
    let synthetic_summary = format!("consolidated@{}", Utc::now().format("%Y-%m-%d"));
    let id = merger::fact_id(page_title, &synthetic_summary);

    let new_block = format!(
        "{open}{id}{close_marker}\n{body}\n{end}",
        open = FACT_OPEN_PREFIX,
        close_marker = FACT_OPEN_SUFFIX,
        body = consolidated.trim(),
        end = FACT_CLOSE,
    );

    let mut out = String::with_capacity(body.len());
    out.push_str(prefix);
    out.push_str(&new_block);
    out.push_str(suffix);
    out
}

fn render_prompt(
    template_path: &Path,
    page_title: &str,
    blocks: &[Block],
) -> Result<RenderedPrompt> {
    let raw = std::fs::read_to_string(template_path)
        .with_context(|| format!("reading consolidate prompt {}", template_path.display()))?;
    let parsed: ConsolidateFile = toml::from_str(&raw).with_context(|| {
        format!(
            "parsing consolidate prompt template {}",
            template_path.display()
        )
    })?;

    let mut env = Environment::new();
    env.add_template("consolidate_user", &parsed.prompt.user_template)
        .context("compiling consolidate user_template")?;
    let template = env.get_template("consolidate_user").unwrap();

    let fragments: Vec<&str> = blocks.iter().map(|b| b.body.as_str()).collect();
    let user = template
        .render(context! { page_title => page_title, fragments => fragments })
        .context("rendering consolidate user_template")?;

    Ok(RenderedPrompt {
        system: parsed.prompt.system,
        user,
        model: parsed.meta.model,
        max_tokens: parsed.meta.max_tokens,
    })
}

/// Strip a single optional ```` ```markdown / ```md / ``` ```` code-fence
/// wrapper the LLM might add around the whole response despite our
/// instructions.
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
    fn extract_blocks_handles_two_blocks() {
        let body = "intro\n<!-- alluvium:fact id=aaaa1111 -->\nfirst\n<!-- alluvium:end -->\ngap\n<!-- alluvium:fact id=bbbb2222 -->\nsecond\n<!-- alluvium:end -->\noutro";
        let blocks = extract_blocks(body);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].body, "first");
        assert_eq!(blocks[1].body, "second");
    }

    #[test]
    fn extract_blocks_returns_empty_when_no_markers() {
        let body = "plain markdown with no markers";
        assert!(extract_blocks(body).is_empty());
    }

    #[test]
    fn extract_blocks_drops_unclosed_marker() {
        let body = "<!-- alluvium:fact id=aaaa1111 -->\nfirst\n<!-- alluvium:end -->\n<!-- alluvium:fact id=bbbb2222 -->\noh-no-no-close-marker";
        let blocks = extract_blocks(body);
        assert_eq!(blocks.len(), 1, "the unclosed orphan should be ignored");
        assert_eq!(blocks[0].body, "first");
    }

    #[test]
    fn replace_blocks_keeps_user_content_outside_envelope() {
        let body = "user heading\n\n<!-- alluvium:fact id=aaaa1111 -->\nfirst\n<!-- alluvium:end -->\n\nuser middle\n\n<!-- alluvium:fact id=bbbb2222 -->\nsecond\n<!-- alluvium:end -->\n\nuser footer";
        let blocks = extract_blocks(body);
        assert_eq!(blocks.len(), 2);
        let out = replace_blocks_with_single(body, &blocks, "Some Topic", "ONE consolidated body");
        assert!(out.contains("user heading"));
        assert!(out.contains("user footer"));
        assert!(out.contains("ONE consolidated body"));
        // Exactly one fact-block in the output.
        let opens = out.matches("<!-- alluvium:fact id=").count();
        let closes = out.matches("<!-- alluvium:end -->").count();
        assert_eq!(opens, 1);
        assert_eq!(closes, 1);
    }

    #[test]
    fn strip_wrapper_unwraps_code_fence() {
        assert_eq!(strip_wrapper("```\nbody\n```"), "body");
        assert_eq!(strip_wrapper("```markdown\nbody\n```"), "body");
        assert_eq!(strip_wrapper("```md\nbody\n```"), "body");
        assert_eq!(strip_wrapper("plain"), "plain");
    }
}
