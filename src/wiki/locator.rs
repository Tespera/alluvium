//! Compute the canonical filesystem path for a fact's topic page, plus the
//! v0.1 fuzzy fallback when the exact slug doesn't already exist.
//!
//! [`page_path`] is pure path arithmetic. [`find_fuzzy_existing`] scans
//! the relevant `wiki/{concepts|entities}/` directory and returns the path
//! of any existing page whose slug or frontmatter title is close enough to
//! the fact's. The new-vs-existing decision composes both in [`super::decide`].

use std::path::{Path, PathBuf};

use crate::extraction::ExtractedFact;

/// Where would `fact`'s topic page live under `alluvium_root`?
///
/// Layout: `<alluvium_root>/wiki/{concepts|entities}/<page_slug>.md`
/// where the subdirectory is determined by [`crate::extraction::FactKind::wiki_subdir`]
/// (decision and gotcha route to `concepts/`).
pub fn page_path(alluvium_root: &Path, fact: &ExtractedFact) -> PathBuf {
    alluvium_root
        .join("wiki")
        .join(fact.kind.wiki_subdir())
        .join(format!("{}.md", fact.page_slug))
}

/// Slug → existing-file mapping when the *slug* differs but the topic is
/// arguably the same. Used by [`super::decide`] before falling back to
/// "new page". Returns `None` if nothing in the subdir is a close enough
/// match (per [`SIMILARITY_THRESHOLD`]).
///
/// We compare the fact against each existing page on two axes — slug and
/// frontmatter title — and take the highest score. This catches:
///
/// - case / punctuation drift (`alluvium-design` vs `Alluvium-Design`)
/// - minor word edits (`hook-payload` vs `hooks-payload`)
/// - title vs slug mismatch when the LLM picks a slightly different slug
///   for the same topic across sessions
///
/// Cross-language matching is intentionally NOT supported (v0.2 work).
pub fn find_fuzzy_existing(alluvium_root: &Path, fact: &ExtractedFact) -> Option<PathBuf> {
    let dir = alluvium_root.join("wiki").join(fact.kind.wiki_subdir());
    let entries = std::fs::read_dir(&dir).ok()?;

    let target_slug = normalize(&fact.page_slug);
    let target_title = normalize(&fact.page_title);

    let mut best: Option<(f32, PathBuf)> = None;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e != "md").unwrap_or(true) {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s,
            None => continue,
        };
        // Skip the exact-slug case; the caller already handled it.
        if stem == fact.page_slug {
            continue;
        }

        let slug_score = similarity(&target_slug, &normalize(stem));

        let title_score = read_frontmatter_title(&path)
            .map(|t| similarity(&target_title, &normalize(&t)))
            .unwrap_or(0.0);

        let score = slug_score.max(title_score);
        let beats_best = match best.as_ref() {
            None => true,
            Some((b, _)) => score > *b,
        };
        if score >= SIMILARITY_THRESHOLD && beats_best {
            best = Some((score, path));
        }
    }

    best.map(|(_, p)| p)
}

/// Match threshold for [`find_fuzzy_existing`]. 0.75 lets through case /
/// punctuation drift AND modest suffix decoration (`alluvium-design` ↔
/// `alluvium-design-doc`) while rejecting unrelated topics: `claude-code`
/// vs `claude-cli` scores 0.73, `alluvium-hooks` vs `alluvium-flow` scores
/// 0.71 — both correctly stay below.
const SIMILARITY_THRESHOLD: f32 = 0.75;

/// Lowercase + collapse `[-_\s]+` to single spaces. Lets us compare a
/// kebab-case slug against a Title Case page title on the same axis.
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

/// 1 − Levenshtein distance / max(len(a), len(b)). Returns `1.0` when both
/// strings are empty (treated as identical). Operates on chars, so non-ASCII
/// is handled correctly even if cross-language matching is rarely useful.
fn similarity(a: &str, b: &str) -> f32 {
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

fn read_frontmatter_title(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let (fm, _body) = crate::vault::frontmatter::parse(&content).ok()?;
    fm.get("title")?.as_str().map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::{FactKind, Relations};

    fn make_fact(kind: FactKind, slug: &str) -> ExtractedFact {
        ExtractedFact {
            kind,
            page_slug: slug.into(),
            page_title: slug.into(),
            summary: "s".into(),
            body_markdown: "b".into(),
            relations: Relations::default(),
            confidence: 1.0,
        }
    }

    #[test]
    fn entity_routes_to_entities_subdir() {
        let p = page_path(
            Path::new("/v/Alluvium"),
            &make_fact(FactKind::Entity, "alluvium"),
        );
        assert_eq!(p, Path::new("/v/Alluvium/wiki/entities/alluvium.md"));
    }

    #[test]
    fn concept_routes_to_concepts_subdir() {
        let p = page_path(
            Path::new("/v/Alluvium"),
            &make_fact(FactKind::Concept, "atomic-write"),
        );
        assert_eq!(p, Path::new("/v/Alluvium/wiki/concepts/atomic-write.md"));
    }

    #[test]
    fn decision_routes_to_concepts_subdir() {
        let p = page_path(
            Path::new("/v/Alluvium"),
            &make_fact(FactKind::Decision, "use-rust"),
        );
        assert!(p.starts_with("/v/Alluvium/wiki/concepts"));
        assert!(p.ends_with("use-rust.md"));
    }

    #[test]
    fn gotcha_routes_to_concepts_subdir() {
        let p = page_path(
            Path::new("/v/Alluvium"),
            &make_fact(FactKind::Gotcha, "session-id-stdin"),
        );
        assert!(p.starts_with("/v/Alluvium/wiki/concepts"));
    }

    #[test]
    fn relative_root_path_handled() {
        // Caller is responsible for canonicalization; we just compose.
        let p = page_path(Path::new("Alluvium"), &make_fact(FactKind::Entity, "x"));
        assert_eq!(p, Path::new("Alluvium/wiki/entities/x.md"));
    }

    #[test]
    fn slug_with_dashes_preserved() {
        let p = page_path(
            Path::new("/v/Alluvium"),
            &make_fact(FactKind::Concept, "claude-code-plugin-payload"),
        );
        assert!(p.ends_with("claude-code-plugin-payload.md"));
    }

    #[test]
    fn normalize_collapses_separators_and_lowercases() {
        assert_eq!(normalize("Alluvium-Design"), "alluvium design");
        assert_eq!(normalize("alluvium_design  doc"), "alluvium design doc");
        assert_eq!(normalize("Hook Payload"), "hook payload");
        assert_eq!(normalize("hook--payload"), "hook payload");
    }

    #[test]
    fn similarity_identical_strings_is_one() {
        assert!((similarity("hook", "hook") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn similarity_completely_different_is_low() {
        assert!(similarity("alluvium", "database") < 0.5);
    }

    #[test]
    fn similarity_case_normalization_via_caller() {
        // similarity itself is case-sensitive; normalize is the caller's job.
        let a = normalize("Alluvium-Design");
        let b = normalize("alluvium design");
        assert!(similarity(&a, &b) > 0.95);
    }

    #[test]
    fn fuzzy_returns_none_for_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let fact = make_fact(FactKind::Concept, "anything");
        assert!(find_fuzzy_existing(dir.path(), &fact).is_none());
    }

    #[test]
    fn fuzzy_skips_exact_slug_match() {
        // exact-slug match is the caller's concern — fuzzy must skip it
        // so we don't return the same path the caller already has.
        let dir = tempfile::tempdir().unwrap();
        let exact = dir.path().join("wiki/concepts/foo.md");
        std::fs::create_dir_all(exact.parent().unwrap()).unwrap();
        std::fs::write(&exact, "---\ntitle: Foo\n---").unwrap();

        let fact = make_fact(FactKind::Concept, "foo");
        // The exact-slug file is on disk, but find_fuzzy_existing should
        // not return it — that's decide()'s exact-match step.
        assert!(find_fuzzy_existing(dir.path(), &fact).is_none());
    }

    #[test]
    fn fuzzy_picks_best_among_multiple_candidates() {
        let dir = tempfile::tempdir().unwrap();
        let close = dir.path().join("wiki/concepts/alluvium-design.md");
        let far = dir.path().join("wiki/concepts/database-schemas.md");
        std::fs::create_dir_all(close.parent().unwrap()).unwrap();
        std::fs::write(&close, "---\ntitle: Alluvium Design\n---").unwrap();
        std::fs::write(&far, "---\ntitle: Database Schemas\n---").unwrap();

        let fact = make_fact(FactKind::Concept, "alluvium-design-v2");
        let got = find_fuzzy_existing(dir.path(), &fact).expect("should match the close one");
        assert_eq!(got, close);
    }
}
