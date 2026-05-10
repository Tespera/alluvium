//! Scan the wiki to enumerate existing topic pages.
//!
//! Used by `cli::archive` to feed an "existing topics" index to the
//! distill prompt — so the LLM can decide whether each new fact belongs
//! to an established topic (reuse the slug → page gets *updated*) or is
//! genuinely uncovered (mint a new slug). This is the single most
//! important mechanism for making Alluvium honor the *compounding*
//! property of [Karpathy's LLM Wiki](../../docs/LLM_WIKI_DOCTRINE.md)
//! — without it, the LLM has no idea what the wiki already contains and
//! every session generates parallel-universe slugs for the same topics.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// One row in the existing-topics index handed to the distill prompt.
///
/// Keep small — this whole list is rendered into the LLM context, so
/// blowing it out by including full page bodies would be wasteful and
/// could clip the actual transcript content under byte caps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicEntry {
    /// Filename stem under `wiki/{concepts,entities}/`. The dedup key.
    pub slug: String,
    /// `concepts` or `entities`. Matches `FactKind::wiki_subdir`.
    pub kind: String,
    /// Frontmatter `title:` field, falls back to slug when absent.
    pub title: String,
    /// One-line summary (≤200 chars). Pulled from the first
    /// `alluvium:fact` block's body, falling back to the page's first
    /// non-empty body line. Designed to be just enough for the LLM to
    /// judge "does this new fact belong to this topic?" without reading
    /// the full page.
    pub summary: String,
}

/// Walk `<alluvium_root>/wiki/{concepts,entities}/*.md` and collect a
/// flat list of [`TopicEntry`]. Returns an empty list (not an error) if
/// the wiki dir does not yet exist — that's the "fresh vault" case.
///
/// Errors only on truly unexpected I/O failures; per-file parse problems
/// are logged at warn level and the page is skipped, matching how the
/// rest of the archive pipeline handles bad inputs.
pub fn scan(alluvium_root: &Path) -> Result<Vec<TopicEntry>> {
    let mut out = Vec::new();
    for sub in ["concepts", "entities"] {
        let dir = alluvium_root.join("wiki").join(sub);
        if !dir.exists() {
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(err) => {
                tracing::warn!(dir = %dir.display(), error = %err, "index_scan: read_dir failed");
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            match read_topic_entry(&path, sub) {
                Ok(Some(t)) => out.push(t),
                Ok(None) => {} // empty file or unparseable — skip silently
                Err(err) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %format!("{err:#}"),
                        "index_scan: skipping unreadable topic page"
                    );
                }
            }
        }
    }
    out.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(out)
}

fn read_topic_entry(path: &Path, kind: &str) -> Result<Option<TopicEntry>> {
    let slug = match path.file_stem().and_then(|s| s.to_str()) {
        Some(s) => s.to_string(),
        None => return Ok(None),
    };
    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Ok(None);
    }

    let (fm, body) = crate::vault::frontmatter::parse(&raw)?;
    let title = fm
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| slug.clone());
    let summary = extract_summary(&body);

    Ok(Some(TopicEntry {
        slug,
        kind: kind.to_string(),
        title,
        summary,
    }))
}

/// Cap on `summary` length. Bigger gives lint's Jaccard scorer more
/// signal but costs ingest-prompt context. 400 strikes a balance: short
/// enough to render 100 entries in <40 KB of prompt, long enough that
/// lint's Jaccard pre-filter can detect cross-language same-topic pairs
/// whose 1–2-sentence summaries phrase the idea differently.
const SUMMARY_BUDGET: usize = 400;
const FACT_OPEN: &str = "<!-- alluvium:fact id=";
const FACT_OPEN_SUFFIX: &str = " -->";
const FACT_CLOSE: &str = "<!-- alluvium:end -->";

/// Pull a one-line summary from a topic-page body. Tries the first
/// alluvium:fact block first; falls back to the first non-empty body
/// line; truncates to [`SUMMARY_BUDGET`] chars on a sensible boundary.
fn extract_summary(body: &str) -> String {
    let inner = first_fact_block_inner(body).or_else(|| first_non_empty_paragraph(body));
    let raw = inner.unwrap_or_default();
    truncate_chars(&raw, SUMMARY_BUDGET)
}

fn first_fact_block_inner(body: &str) -> Option<String> {
    let open = body.find(FACT_OPEN)?;
    let after_prefix = open + FACT_OPEN.len();
    let suffix_rel = body[after_prefix..].find(FACT_OPEN_SUFFIX)?;
    let block_start = after_prefix + suffix_rel + FACT_OPEN_SUFFIX.len();
    let close_rel = body[block_start..].find(FACT_CLOSE)?;
    let inner = body[block_start..block_start + close_rel].trim();
    if inner.is_empty() {
        return None;
    }
    // Collapse to a single line — multi-paragraph bodies are common but
    // we want one tight sentence for the index.
    Some(collapse_whitespace(inner))
}

fn first_non_empty_paragraph(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("<!--"))
        .map(collapse_whitespace)
        .next()
        .filter(|s| !s.is_empty())
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

/// Truncate a string to `max` *characters* (not bytes). Adds an ellipsis
/// when actually clipping. UTF-8 safe by construction.
fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Pretty-print the index for embedding into the distill prompt's user
/// template. One topic per line, grouped by kind. Stable ordering.
pub fn render_for_prompt(entries: &[TopicEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut concepts: Vec<&TopicEntry> = entries.iter().filter(|t| t.kind == "concepts").collect();
    let mut entities: Vec<&TopicEntry> = entries.iter().filter(|t| t.kind == "entities").collect();
    concepts.sort_by(|a, b| a.slug.cmp(&b.slug));
    entities.sort_by(|a, b| a.slug.cmp(&b.slug));

    let mut out = String::new();
    if !concepts.is_empty() {
        out.push_str("Concepts:\n");
        for t in &concepts {
            out.push_str(&format!("- `{}` — {}: {}\n", t.slug, t.title, t.summary));
        }
    }
    if !entities.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str("Entities:\n");
        for t in &entities {
            out.push_str(&format!("- `{}` — {}: {}\n", t.slug, t.title, t.summary));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, content: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }

    #[test]
    fn scan_returns_empty_for_missing_wiki() {
        let dir = tempfile::tempdir().unwrap();
        let topics = scan(dir.path()).unwrap();
        assert!(topics.is_empty());
    }

    #[test]
    fn scan_collects_concepts_and_entities() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("wiki/concepts/atomic-write.md"),
            "---\ntitle: Atomic File Write\n---\n\n# Atomic File Write\n\n<!-- alluvium:fact id=11111111 -->\nWrite to tmp, fsync, rename. POSIX guarantees atomicity.\n<!-- alluvium:end -->\n",
        );
        write(
            &dir.path().join("wiki/entities/alluvium.md"),
            "---\ntitle: Alluvium\n---\n\n# Alluvium\n\n<!-- alluvium:fact id=22222222 -->\nA Claude Code session auto-archiver.\n<!-- alluvium:end -->\n",
        );
        let topics = scan(dir.path()).unwrap();
        assert_eq!(topics.len(), 2);

        let atomic = topics.iter().find(|t| t.slug == "atomic-write").unwrap();
        assert_eq!(atomic.kind, "concepts");
        assert_eq!(atomic.title, "Atomic File Write");
        assert!(atomic.summary.contains("Write to tmp"));

        let alluvium = topics.iter().find(|t| t.slug == "alluvium").unwrap();
        assert_eq!(alluvium.kind, "entities");
    }

    #[test]
    fn summary_falls_back_to_first_paragraph_when_no_marker() {
        let body = "## Heading\n\nFirst real paragraph here.\nSecond line.\n";
        let s = extract_summary(body);
        assert!(
            s.starts_with("First real paragraph"),
            "expected fallback; got: {s}"
        );
    }

    #[test]
    fn summary_truncates_overly_long_marker_body() {
        let long = "A".repeat(500);
        let body = format!("<!-- alluvium:fact id=11111111 -->\n{long}\n<!-- alluvium:end -->\n");
        let s = extract_summary(&body);
        assert!(s.chars().count() <= SUMMARY_BUDGET);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn summary_collapses_multiline_marker_body() {
        let body = "<!-- alluvium:fact id=11111111 -->\nLine one.\n\nLine two.\n  Line three.\n<!-- alluvium:end -->\n";
        let s = extract_summary(body);
        assert_eq!(s, "Line one. Line two. Line three.");
    }

    #[test]
    fn render_groups_concepts_and_entities_distinctly() {
        let entries = vec![
            TopicEntry {
                slug: "foo".into(),
                kind: "concepts".into(),
                title: "Foo".into(),
                summary: "Foo summary".into(),
            },
            TopicEntry {
                slug: "bar".into(),
                kind: "entities".into(),
                title: "Bar".into(),
                summary: "Bar summary".into(),
            },
        ];
        let rendered = render_for_prompt(&entries);
        assert!(rendered.contains("Concepts:"));
        assert!(rendered.contains("Entities:"));
        assert!(rendered.contains("`foo`"));
        assert!(rendered.contains("`bar`"));
    }

    #[test]
    fn render_empty_list_yields_empty_string() {
        assert!(render_for_prompt(&[]).is_empty());
    }

    #[test]
    fn scan_skips_non_md_files() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join("wiki/concepts/foo.md"),
            "---\ntitle: Foo\n---\n\nbody\n",
        );
        write(&dir.path().join("wiki/concepts/.DS_Store"), "garbage");
        write(&dir.path().join("wiki/concepts/notes.txt"), "not markdown");
        let topics = scan(dir.path()).unwrap();
        assert_eq!(topics.len(), 1);
        assert_eq!(topics[0].slug, "foo");
    }

    #[test]
    fn truncate_handles_unicode_correctly() {
        // 5 chars (CJK), each 3 bytes — naive byte-truncation would cut mid-codepoint.
        let s = "原子文件写入";
        let t = truncate_chars(s, 4);
        assert_eq!(t.chars().count(), 4);
        assert!(t.ends_with('…'));
    }

    #[test]
    fn first_fact_block_handles_two_blocks_picks_first() {
        let body = "<!-- alluvium:fact id=AAAA -->\nFirst.\n<!-- alluvium:end -->\n<!-- alluvium:fact id=BBBB -->\nSecond.\n<!-- alluvium:end -->\n";
        let s = extract_summary(body);
        assert!(s.contains("First"));
        assert!(!s.contains("Second"));
    }
}
