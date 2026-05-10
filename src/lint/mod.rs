//! `alluvium lint` — periodic health check that fixes wiki entropy.
//!
//! Per [LLM_WIKI_DOCTRINE](../../docs/LLM_WIKI_DOCTRINE.md) principle 3:
//! a wiki without lint is a wiki that rots. Even with wiki-aware ingest,
//! cross-language drift, synonym variation, and the occasional LLM
//! brain-fart will sneak duplicate slugs in. Lint catches them.
//!
//! ## v0.1 algorithm
//!
//! 1. **Scan** all `wiki/{concepts,entities}/*.md` topic pages.
//! 2. **Pair-score** each `(A, B)` with `A < B` using slug + title fuzzy
//!    similarity. Pairs above [`CANDIDATE_THRESHOLD`] are candidates.
//! 3. **Ask the LLM** to decide MERGE vs KEEP for each candidate, with a
//!    rewritten merged body when merging.
//! 4. **Apply** (when `--apply` is set): write the merged body into the
//!    winning slug's file, delete the loser file, append a `lint`
//!    entry to `wiki/log.md`. Dry-run prints the decisions only.
//!
//! ## Why pairwise instead of clustering
//!
//! Pairwise is O(n²) but transparent — each LLM call sees exactly the
//! two candidate pages. Clustering (group of 3+ near-duplicates → one
//! merged page) is harder to reason about and harder to test. v0.2 may
//! add it; v0.1 keeps it simple. With 500 topics that's 125k pairs in
//! the worst case, but the fuzzy filter typically reduces that to <50
//! candidate pairs even on a messy vault.

use anyhow::{Context, Result};
use chrono::Utc;
use minijinja::{context, Environment};
use serde::Deserialize;
use std::path::{Path, PathBuf};

use crate::distiller::backend::{LlmBackend, RenderedPrompt};
use crate::vault::{frontmatter, writer};
use crate::wiki::index_scan;

/// Pairs scoring at or above this go to the LLM. Lower than the ingest
/// fuzzy threshold (0.75) — lint should be more aggressive about
/// surfacing candidates because the LLM has the final say (and can
/// always vote KEEP).
const CANDIDATE_THRESHOLD: f32 = 0.65;

/// One candidate pair to send to the LLM.
#[derive(Debug, Clone)]
pub struct Pair {
    pub path_a: PathBuf,
    pub path_b: PathBuf,
    pub slug_a: String,
    pub slug_b: String,
    pub title_a: String,
    pub title_b: String,
    pub score: f32,
}

/// What the LLM decided about a pair.
#[derive(Debug, Clone, Deserialize)]
pub struct LintDecision {
    pub decision: String, // "merge" | "keep"
    #[serde(default)]
    pub winning_slug: Option<String>,
    #[serde(default)]
    pub merged_body: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

/// One row in the dry-run report.
#[derive(Debug)]
pub struct LintReportRow {
    pub pair: Pair,
    pub decision: LintDecision,
    pub applied: bool,
}

/// Top-level summary returned to the CLI handler.
#[derive(Debug)]
pub struct LintReport {
    pub rows: Vec<LintReportRow>,
}

impl LintReport {
    pub fn merged_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.decision.decision == "merge" && r.applied)
            .count()
    }
    pub fn merge_decided_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.decision.decision == "merge")
            .count()
    }
    pub fn kept_count(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| r.decision.decision == "keep")
            .count()
    }
}

/// Walk the wiki, score every page-pair, return those above
/// [`CANDIDATE_THRESHOLD`]. Pairs come back in stable order: each page
/// appears at most once on the left (path_a < path_b lexicographically).
pub fn collect_candidates(alluvium_root: &Path) -> Result<Vec<Pair>> {
    let topics = index_scan::scan(alluvium_root)?;

    let mut pairs = Vec::new();
    for i in 0..topics.len() {
        for j in (i + 1)..topics.len() {
            let a = &topics[i];
            let b = &topics[j];
            // Cross-kind pairs (concept vs entity) are intentionally
            // included — language drift sometimes pushes the same topic
            // into the wrong subdir. The LLM can decide.
            let score = score_pair(a, b);
            if score >= CANDIDATE_THRESHOLD {
                pairs.push(Pair {
                    path_a: page_path(alluvium_root, a),
                    path_b: page_path(alluvium_root, b),
                    slug_a: a.slug.clone(),
                    slug_b: b.slug.clone(),
                    title_a: a.title.clone(),
                    title_b: b.title.clone(),
                    score,
                });
            }
        }
    }
    Ok(pairs)
}

/// Run lint end-to-end: collect candidates → ask LLM for each → maybe
/// apply. Returns the report so the CLI can print it.
pub async fn run_lint(
    alluvium_root: &Path,
    backend: &dyn LlmBackend,
    prompt_template_path: &Path,
    apply: bool,
) -> Result<LintReport> {
    let candidates = collect_candidates(alluvium_root)?;
    let mut rows = Vec::with_capacity(candidates.len());

    for pair in candidates {
        let decision = match decide_pair(&pair, backend, prompt_template_path).await {
            Ok(d) => d,
            Err(err) => {
                tracing::warn!(
                    slug_a = %pair.slug_a,
                    slug_b = %pair.slug_b,
                    error = %format!("{err:#}"),
                    "lint: LLM decision failed; treating as keep"
                );
                LintDecision {
                    decision: "keep".into(),
                    winning_slug: None,
                    merged_body: None,
                    reason: Some(format!("LLM error: {err}")),
                }
            }
        };

        let applied = if apply && decision.decision == "merge" {
            match apply_merge(alluvium_root, &pair, &decision) {
                Ok(()) => true,
                Err(err) => {
                    tracing::error!(
                        slug_a = %pair.slug_a,
                        slug_b = %pair.slug_b,
                        error = %format!("{err:#}"),
                        "lint: apply failed; pair left untouched"
                    );
                    false
                }
            }
        } else {
            false
        };

        rows.push(LintReportRow {
            pair,
            decision,
            applied,
        });
    }

    Ok(LintReport { rows })
}

fn page_path(alluvium_root: &Path, t: &index_scan::TopicEntry) -> PathBuf {
    alluvium_root
        .join("wiki")
        .join(&t.kind)
        .join(format!("{}.md", t.slug))
}

/// Score two topics for "are they near-duplicates?". Mirrors the ingest
/// fuzzy heuristic (slug + title, normalized Levenshtein) but the
/// threshold is looser since the LLM provides the safety net.
fn score_pair(a: &index_scan::TopicEntry, b: &index_scan::TopicEntry) -> f32 {
    let slug_score = string_similarity(&normalize(&a.slug), &normalize(&b.slug));
    let title_score = string_similarity(&normalize(&a.title), &normalize(&b.title));
    slug_score.max(title_score)
}

fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == '-' || ch == '_' || ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.extend(ch.to_lowercase());
            prev_space = false;
        }
    }
    out.trim().to_string()
}

fn string_similarity(a: &str, b: &str) -> f32 {
    let av: Vec<char> = a.chars().collect();
    let bv: Vec<char> = b.chars().collect();
    let max = av.len().max(bv.len());
    if max == 0 {
        return 1.0;
    }
    let dist = levenshtein(&av, &bv);
    1.0 - (dist as f32 / max as f32)
}

fn levenshtein(a: &[char], b: &[char]) -> usize {
    let (m, n) = (a.len(), b.len());
    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut cur = vec![0usize; n + 1];
    for i in 1..=m {
        cur[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (cur[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[n]
}

#[derive(Debug, Deserialize)]
struct LintFile {
    meta: LintMeta,
    prompt: LintPrompt,
}

#[derive(Debug, Deserialize)]
struct LintMeta {
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
struct LintPrompt {
    system: String,
    user_template: String,
}

async fn decide_pair(
    pair: &Pair,
    backend: &dyn LlmBackend,
    prompt_template_path: &Path,
) -> Result<LintDecision> {
    let raw = std::fs::read_to_string(prompt_template_path).with_context(|| {
        format!(
            "reading lint prompt template {}",
            prompt_template_path.display()
        )
    })?;
    let parsed: LintFile = toml::from_str(&raw).with_context(|| {
        format!(
            "parsing lint prompt template {}",
            prompt_template_path.display()
        )
    })?;

    let body_a = std::fs::read_to_string(&pair.path_a)
        .with_context(|| format!("reading {}", pair.path_a.display()))?;
    let body_b = std::fs::read_to_string(&pair.path_b)
        .with_context(|| format!("reading {}", pair.path_b.display()))?;

    let mut env = Environment::new();
    env.add_template("lint_user", &parsed.prompt.user_template)
        .context("compiling lint user_template")?;
    let template = env.get_template("lint_user").unwrap();

    let user = template
        .render(context! {
            slug_a => pair.slug_a,
            slug_b => pair.slug_b,
            title_a => pair.title_a,
            title_b => pair.title_b,
            body_a => body_a,
            body_b => body_b,
        })
        .context("rendering lint user_template")?;

    let prompt = RenderedPrompt {
        system: parsed.prompt.system,
        user,
        model: parsed.meta.model,
        max_tokens: parsed.meta.max_tokens,
    };

    let response = backend
        .complete(&prompt)
        .await
        .context("calling LLM backend for lint")?;
    let decision = parse_decision(&response.text)?;
    Ok(decision)
}

/// Parse the LLM's response into a [`LintDecision`]. Tolerant of
/// code-fence wrapping and surrounding prose, mirroring the distiller's
/// own JSON parser.
fn parse_decision(text: &str) -> Result<LintDecision> {
    let unfenced = unwrap_code_fence(text);
    let json_str = locate_json_object(unfenced)
        .context("could not locate a JSON object in lint LLM output")?;
    let decision: LintDecision = serde_json::from_str(json_str).with_context(|| {
        let preview: String = json_str.chars().take(500).collect();
        format!("lint output was not valid JSON; first 500 chars: {preview}")
    })?;

    if decision.decision != "merge" && decision.decision != "keep" {
        anyhow::bail!(
            "lint LLM returned unexpected decision {:?}; expected 'merge' or 'keep'",
            decision.decision
        );
    }
    if decision.decision == "merge"
        && (decision.winning_slug.is_none() || decision.merged_body.is_none())
    {
        anyhow::bail!(
            "lint LLM said 'merge' but omitted winning_slug or merged_body — treating as keep"
        );
    }
    Ok(decision)
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

/// Apply a MERGE decision: write the merged body into the winning page,
/// delete the losing page, append a lint entry to log.md.
fn apply_merge(alluvium_root: &Path, pair: &Pair, decision: &LintDecision) -> Result<()> {
    let winner_slug = decision
        .winning_slug
        .as_deref()
        .context("apply_merge called on decision without winning_slug")?;
    let merged_body = decision
        .merged_body
        .as_deref()
        .context("apply_merge called on decision without merged_body")?;

    let (winner_path, loser_path) = if pair.slug_a == winner_slug {
        (&pair.path_a, &pair.path_b)
    } else if pair.slug_b == winner_slug {
        (&pair.path_b, &pair.path_a)
    } else {
        anyhow::bail!(
            "lint LLM picked a winning_slug ({winner_slug:?}) that matches neither {:?} nor {:?}",
            pair.slug_a,
            pair.slug_b
        );
    };

    // Read the winner's existing frontmatter so we keep created/updated/_alluvium
    // bookkeeping correct rather than dropping it.
    let winner_raw = std::fs::read_to_string(winner_path)
        .with_context(|| format!("reading winner page {}", winner_path.display()))?;
    let (mut fm, _old_body) = frontmatter::parse(&winner_raw)?;
    if let Some(map) = fm.as_mapping_mut() {
        map.insert(
            serde_yaml::Value::String("updated".into()),
            serde_yaml::Value::String(Utc::now().format("%Y-%m-%d").to_string()),
        );
    }

    // The merged_body is *just* the markdown body. Wrap it in one
    // alluvium:fact block (consolidate-style) so future archives know
    // this region is Alluvium-owned.
    let synthetic_summary = format!("lint-merged@{}", Utc::now().format("%Y-%m-%d"));
    let id = crate::vault::merger::fact_id(winner_slug, &synthetic_summary);
    let body = format!(
        "# {title}\n\n<!-- alluvium:fact id={id} -->\n{body}\n<!-- alluvium:end -->\n",
        title = winner_title(&fm, winner_slug),
        body = merged_body.trim(),
    );
    let new_content = frontmatter::render(&fm, &body)?;
    writer::write_atomic(winner_path, &new_content)?;

    // Delete the loser. Best-effort — if the file is gone for some
    // reason, that's fine.
    if loser_path.exists() {
        std::fs::remove_file(loser_path)
            .with_context(|| format!("removing merged-away topic page {}", loser_path.display()))?;
    }

    // Append a lint entry to log.md so users (and future lint passes)
    // can see what happened. Format follows the Karpathy gist's
    // convention: `## [YYYY-MM-DD] lint | <one-line summary>`.
    let log_path = alluvium_root.join("wiki").join("log.md");
    let entry = format!(
        "\n## [{date}] lint | merged `{loser}` → `{winner}`\n  Reason: {reason}\n",
        date = Utc::now().format("%Y-%m-%d"),
        loser = if pair.slug_a == winner_slug {
            &pair.slug_b
        } else {
            &pair.slug_a
        },
        winner = winner_slug,
        reason = decision.reason.as_deref().unwrap_or("(no reason provided)"),
    );
    append_log(&log_path, &entry)?;

    Ok(())
}

fn winner_title(fm: &serde_yaml::Value, fallback_slug: &str) -> String {
    fm.get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| fallback_slug.to_string())
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

    fn topic(slug: &str, kind: &str, title: &str) -> index_scan::TopicEntry {
        index_scan::TopicEntry {
            slug: slug.into(),
            kind: kind.into(),
            title: title.into(),
            summary: String::new(),
        }
    }

    #[test]
    fn score_high_for_typo_variant() {
        let a = topic("github-cdn-409-conflict", "concepts", "GitHub CDN 409");
        let b = topic("gridea-cnd-409-conflict", "concepts", "Gridea CND 409");
        let s = score_pair(&a, &b);
        assert!(s >= CANDIDATE_THRESHOLD, "expected to fire, got {s}");
    }

    #[test]
    fn score_low_for_unrelated() {
        let a = topic("alluvium-design", "concepts", "Alluvium Design");
        let b = topic("database-schemas", "concepts", "Database Schemas");
        let s = score_pair(&a, &b);
        assert!(s < CANDIDATE_THRESHOLD, "expected no match, got {s}");
    }

    #[test]
    fn score_high_when_title_matches_even_if_slugs_differ() {
        // slug language drift: same concept, different scripts.
        // (Cross-language slugs alone won't fuzzy-match — we depend on
        //  the user keeping `title` consistent if they want lint to
        //  catch this. v0.2 will use embeddings.)
        let a = index_scan::TopicEntry {
            slug: "atomic-write".into(),
            kind: "concepts".into(),
            title: "Atomic Write".into(),
            summary: String::new(),
        };
        let b = index_scan::TopicEntry {
            slug: "atomic-file-write".into(),
            kind: "concepts".into(),
            title: "Atomic Write".into(), // identical title
            summary: String::new(),
        };
        let s = score_pair(&a, &b);
        assert!(s >= CANDIDATE_THRESHOLD);
    }

    #[test]
    fn parse_decision_accepts_well_formed_merge() {
        let json = r#"{"decision":"merge","winning_slug":"foo","merged_body":"body","reason":"same topic"}"#;
        let d = parse_decision(json).unwrap();
        assert_eq!(d.decision, "merge");
        assert_eq!(d.winning_slug.as_deref(), Some("foo"));
    }

    #[test]
    fn parse_decision_accepts_keep_with_nulls() {
        let json =
            r#"{"decision":"keep","winning_slug":null,"merged_body":null,"reason":"different"}"#;
        let d = parse_decision(json).unwrap();
        assert_eq!(d.decision, "keep");
        assert!(d.winning_slug.is_none());
    }

    #[test]
    fn parse_decision_unwraps_code_fence() {
        let json = "```json\n{\"decision\":\"keep\"}\n```";
        let d = parse_decision(json).unwrap();
        assert_eq!(d.decision, "keep");
    }

    #[test]
    fn parse_decision_rejects_unknown_decision() {
        let json = r#"{"decision":"maybe"}"#;
        assert!(parse_decision(json).is_err());
    }

    #[test]
    fn parse_decision_rejects_merge_missing_fields() {
        let json = r#"{"decision":"merge","winning_slug":"foo"}"#;
        assert!(parse_decision(json).is_err());
    }
}
