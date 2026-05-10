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

/// Per-axis thresholds for "is this pair a candidate?". A pair fires if
/// **any single axis** crosses its threshold — they're independent
/// signals, not conditions. The LLM has the final MERGE/KEEP say, so
/// we calibrate aggressively on the surfacing side.
///
/// Three body axes (any one fires the pair):
///   - **bigram Jaccard** on the summary string. Catches typos and
///     same-language paraphrases. Loose threshold because short
///     summaries have noisy bigram distributions.
///   - **CJK-char Jaccard** on summary chars in U+4E00..=U+9FFF. Each
///     CJK character is roughly word-level, so single-char overlap is
///     a far stronger signal than ASCII single-char. This is the
///     primary catch-net for Chinese same-topic pairs that phrase the
///     idea differently — empirically the 95/5 vault cluster lands
///     here (≈0.45 char Jaccard despite ≈0.10 bigram Jaccard).
///   - **ASCII-word Jaccard** on whitespace-split tokens ≥3 chars,
///     lowercased. Mirror of the CJK axis for English/Latin scripts.
const SLUG_TITLE_THRESHOLD: f32 = 0.65;
const BIGRAM_JACCARD_THRESHOLD: f32 = 0.20;
// CJK 0.10 looks low but is calibrated to a real-vault observation: same-
// topic pages with rephrased prose (the "95/5 deploy UI" cluster) score
// 0.10–0.15 on CJK char Jaccard while clearly-unrelated pairs sit
// 0.04–0.08. The LLM has the final say on MERGE/KEEP, so a permissive
// pre-filter trades cost for recall — better to spend a few extra LLM
// calls than to miss obvious semantic duplicates.
const CJK_CHAR_JACCARD_THRESHOLD: f32 = 0.10;
const ASCII_WORD_JACCARD_THRESHOLD: f32 = 0.30;

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
            if let Some(score) = candidate_score(a, b) {
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

    // Pages that have been merged away during this run. Subsequent pairs
    // touching one of these would error out (file gone) — skip them
    // cleanly with a "superseded by earlier merge" reason instead.
    let mut deleted: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for pair in candidates {
        if deleted.contains(&pair.path_a) || deleted.contains(&pair.path_b) {
            rows.push(LintReportRow {
                pair: pair.clone(),
                decision: LintDecision {
                    decision: "keep".into(),
                    winning_slug: None,
                    merged_body: None,
                    reason: Some(
                        "skipped — one of the pages was merged away earlier in this run".into(),
                    ),
                },
                applied: false,
            });
            continue;
        }

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
                Ok(()) => {
                    // Track which page was deleted so subsequent pairs
                    // touching it skip the LLM round-trip.
                    if let Some(winner_slug) = decision.winning_slug.as_deref() {
                        let loser_path = if pair.slug_a == winner_slug {
                            pair.path_b.clone()
                        } else {
                            pair.path_a.clone()
                        };
                        deleted.insert(loser_path);
                    }
                    true
                }
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

/// Decide whether two topics are similar enough to send to the LLM.
/// Returns `Some(reported_score)` if so, `None` otherwise.
///
/// Three independent axes — each has its own threshold because the
/// score distributions are not comparable:
///
/// 1. **Slug Levenshtein** (≥ [`SLUG_TITLE_THRESHOLD`] = 0.65).
/// 2. **Title Levenshtein** (≥ [`SLUG_TITLE_THRESHOLD`] = 0.65).
/// 3. **Body bigram Jaccard** on the `summary` text
///    (≥ [`BODY_JACCARD_THRESHOLD`] = 0.30). This is the cross-language
///    rescue — same-topic pairs whose slugs/titles are in different
///    scripts can have near-zero string similarity but still share
///    enough technical bigrams in their summaries to fire. Without
///    this axis, every cross-language duplicate slips past lint.
///
/// The reported score (for the report column) is the MAX across axes,
/// so the operator's eye lands on the strongest signal first.
fn candidate_score(a: &index_scan::TopicEntry, b: &index_scan::TopicEntry) -> Option<f32> {
    let slug_score = string_similarity(&normalize(&a.slug), &normalize(&b.slug));
    let title_score = string_similarity(&normalize(&a.title), &normalize(&b.title));
    let bigram_score = bigram_jaccard(&a.summary, &b.summary);
    let cjk_score = cjk_char_jaccard(&a.summary, &b.summary);
    let ascii_score = ascii_word_jaccard(&a.summary, &b.summary);

    let fires = slug_score >= SLUG_TITLE_THRESHOLD
        || title_score >= SLUG_TITLE_THRESHOLD
        || bigram_score >= BIGRAM_JACCARD_THRESHOLD
        || cjk_score >= CJK_CHAR_JACCARD_THRESHOLD
        || ascii_score >= ASCII_WORD_JACCARD_THRESHOLD;
    if fires {
        Some(
            slug_score
                .max(title_score)
                .max(bigram_score)
                .max(cjk_score)
                .max(ascii_score),
        )
    } else {
        None
    }
}

/// Jaccard over the *set* of CJK code points (U+4E00..=U+9FFF) in each
/// string. Tuned for Chinese / Japanese kanji where individual characters
/// carry word-level meaning, so a single-char set Jaccard is a much
/// stronger signal than bigram Jaccard.
fn cjk_char_jaccard(a: &str, b: &str) -> f32 {
    let sa: std::collections::HashSet<char> = a.chars().filter(is_cjk).collect();
    let sb: std::collections::HashSet<char> = b.chars().filter(is_cjk).collect();
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn is_cjk(c: &char) -> bool {
    matches!(*c, '\u{4E00}'..='\u{9FFF}')
}

/// Jaccard over lowercase ASCII word tokens of length ≥3. Mirrors
/// [`cjk_char_jaccard`] for Latin-script content. Filtering short tokens
/// drops "the" / "and" / "a" stopwords that would otherwise blow out
/// the union and depress the score for any English text.
fn ascii_word_jaccard(a: &str, b: &str) -> f32 {
    let sa = ascii_word_set(a);
    let sb = ascii_word_set(b);
    if sa.is_empty() || sb.is_empty() {
        return 0.0;
    }
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn ascii_word_set(s: &str) -> std::collections::HashSet<String> {
    s.split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
        .filter(|t| t.len() >= 3 && t.is_ascii())
        .map(|t| t.to_lowercase())
        .collect()
}

/// Jaccard similarity over character bigrams: `|A∩B| / |A∪B|`.
///
/// Char bigrams (not word tokens) work for both English ("at-om" / "to-mi"
/// / "om-ic") and CJK ("原-子" / "子-写" / "写-入") without needing a
/// language-aware tokenizer. Empty inputs score 0 (treated as "no overlap"
/// — better than 1.0, which would false-positive on every empty-summary
/// pair). Returns the *coefficient* in [0,1].
fn bigram_jaccard(a: &str, b: &str) -> f32 {
    let ba = bigram_set(a);
    let bb = bigram_set(b);
    if ba.is_empty() || bb.is_empty() {
        return 0.0;
    }
    let intersection = ba.intersection(&bb).count();
    let union = ba.union(&bb).count();
    if union == 0 {
        return 0.0;
    }
    intersection as f32 / union as f32
}

fn bigram_set(s: &str) -> std::collections::HashSet<(char, char)> {
    let chars: Vec<char> = s
        .chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .filter(|c| !c.is_whitespace())
        .collect();
    let mut out = std::collections::HashSet::with_capacity(chars.len().saturating_sub(1));
    for w in chars.windows(2) {
        out.insert((w[0], w[1]));
    }
    out
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
    fn fires_for_slug_typo_variant() {
        let a = topic("github-cdn-409-conflict", "concepts", "GitHub CDN 409");
        let b = topic("gridea-cnd-409-conflict", "concepts", "Gridea CND 409");
        assert!(
            candidate_score(&a, &b).is_some(),
            "expected pair to fire on slug Levenshtein"
        );
    }

    #[test]
    fn does_not_fire_for_unrelated() {
        let a = topic("alluvium-design", "concepts", "Alluvium Design");
        let b = topic("database-schemas", "concepts", "Database Schemas");
        assert!(
            candidate_score(&a, &b).is_none(),
            "unrelated topics must not fire"
        );
    }

    fn topic_with_summary(
        slug: &str,
        kind: &str,
        title: &str,
        summary: &str,
    ) -> index_scan::TopicEntry {
        index_scan::TopicEntry {
            slug: slug.into(),
            kind: kind.into(),
            title: title.into(),
            summary: summary.into(),
        }
    }

    /// The cross-language case that motivated body Jaccard scoring:
    /// two pages about the same UI design principle, one Chinese-titled
    /// and one English-titled. Slug + title fuzzy alone says "different";
    /// body Jaccard catches the shared technical vocabulary.
    #[test]
    fn fires_for_cross_language_via_body_jaccard() {
        let a = topic_with_summary(
            "ui-design-95-percent-principle",
            "concepts",
            "UI Design 95% Principle",
            "Optimize the deployment UI for the 95% success path. Failures get visible UI; success stays minimal — no permanent panel cluttering the screen.",
        );
        let b = topic_with_summary(
            "部署-UI-95-原则-错误框架",
            "concepts",
            "部署 UI 95 原则",
            "Optimize the deployment UI for the 95% success path. Failures get visible UI; success stays minimal — no permanent panel cluttering the screen.",
        );
        assert!(
            candidate_score(&a, &b).is_some(),
            "cross-language pair with similar body should fire"
        );
    }

    #[test]
    fn jaccard_returns_zero_for_empty() {
        assert_eq!(bigram_jaccard("", ""), 0.0);
        assert_eq!(bigram_jaccard("abc", ""), 0.0);
    }

    #[test]
    fn jaccard_returns_one_for_identical() {
        assert!((bigram_jaccard("hello world", "hello world") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn jaccard_low_for_unrelated() {
        let s = bigram_jaccard(
            "the quick brown fox jumps over the lazy dog",
            "lorem ipsum dolor sit amet consectetur adipiscing",
        );
        assert!(s < 0.2, "unrelated text should score <0.2; got {s}");
    }

    #[test]
    fn cjk_char_jaccard_above_threshold_for_same_topic_differently_phrased() {
        // Real-world cluster signature: two summaries about the same UI
        // design principle, both Chinese, very different prose. CJK
        // single-char Jaccard sits around 0.10–0.15 — the threshold was
        // calibrated to land just below this.
        let a =
            "我们讨论了多个部署面板方案，转折点是用户95%的时间不想看日志。UI的存在本身就是打扰。";
        let b =
            "尝试了5种UI方案，每一种都因为在95%成功路径上强行放一块UI而被否决，对用户是认知负担。";
        let s = cjk_char_jaccard(a, b);
        assert!(
            s >= CJK_CHAR_JACCARD_THRESHOLD,
            "same-topic Chinese pair should fire CJK Jaccard, got {s}"
        );
    }

    #[test]
    fn cjk_char_jaccard_below_threshold_for_unrelated_chinese() {
        let a = "数据库设计需要考虑事务隔离级别和并发读写。";
        let b = "前端组件的样式应该走 Tailwind 而不是写自定义 CSS。";
        let s = cjk_char_jaccard(a, b);
        assert!(
            s < CJK_CHAR_JACCARD_THRESHOLD,
            "unrelated Chinese pair must not fire CJK Jaccard, got {s}"
        );
    }

    #[test]
    fn cjk_char_jaccard_zero_for_pure_ascii() {
        let s = cjk_char_jaccard("hello world", "good morning");
        assert_eq!(s, 0.0);
    }

    #[test]
    fn ascii_word_jaccard_high_for_shared_keywords() {
        let a = "Rust ownership rules prevent data races at compile time.";
        let b = "Rust ownership and borrow checker prevent races at compile time.";
        let s = ascii_word_jaccard(a, b);
        assert!(s >= ASCII_WORD_JACCARD_THRESHOLD, "got {s}");
    }

    #[test]
    fn ascii_word_jaccard_drops_short_stopwords() {
        // The two sentences share only "the" / "and" / "a" — all length
        // <3 so they're filtered out. Score should be near zero.
        let a = "the cat sat on the mat";
        let b = "and a dog ran in the rain";
        let s = ascii_word_jaccard(a, b);
        assert!(s < 0.2, "stopword-only overlap should not fire; got {s}");
    }

    #[test]
    fn fires_when_title_matches_even_if_slugs_differ() {
        // slug language drift: same concept, different scripts.
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
        assert!(candidate_score(&a, &b).is_some());
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
