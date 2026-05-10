//! Decide whether a fact targets a new or existing topic page.
//!
//! Touches the filesystem once (`Path::exists`) to flip between
//! [`TargetPage::Existing`] and [`TargetPage::New`].

use std::path::Path;

use crate::extraction::ExtractedFact;

use super::TargetPage;

/// Compute the [`TargetPage`] for this fact under `alluvium_root`.
///
/// Resolution order:
/// 1. **Exact slug** — `wiki/<subdir>/<slug>.md` exists → `Existing`.
/// 2. **Fuzzy slug or title** — scan the same subdir for a close match
///    (per [`super::locator::find_fuzzy_existing`]) → `Existing`.
/// 3. Otherwise → `New` at the canonical exact-slug path.
///
/// The fuzzy step lets a user's page survive the LLM picking a slightly
/// different slug across sessions for the same topic.
pub fn decide(alluvium_root: &Path, fact: &ExtractedFact) -> TargetPage {
    let path = super::locator::page_path(alluvium_root, fact);
    if path.exists() {
        return TargetPage::Existing(path);
    }
    if let Some(existing) = super::locator::find_fuzzy_existing(alluvium_root, fact) {
        return TargetPage::Existing(existing);
    }
    TargetPage::New(path)
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
    fn missing_page_decides_new() {
        let dir = tempfile::tempdir().unwrap();
        let r = decide(dir.path(), &make_fact(FactKind::Concept, "nonexistent"));
        assert!(r.is_new());
        assert!(r.path().ends_with("wiki/concepts/nonexistent.md"));
    }

    #[test]
    fn existing_page_decides_existing() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("wiki/concepts/here.md");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "# Here").unwrap();

        let r = decide(dir.path(), &make_fact(FactKind::Concept, "here"));
        assert!(!r.is_new());
        assert_eq!(r.path(), target);
    }

    #[test]
    fn entity_existing_in_entities_subdir_detected() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("wiki/entities/alluvium.md");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "x").unwrap();

        let r = decide(dir.path(), &make_fact(FactKind::Entity, "alluvium"));
        assert!(!r.is_new());
        assert_eq!(r.path(), target);
    }

    #[test]
    fn cross_type_does_not_match() {
        // A page in entities/ should not satisfy a fact whose kind routes
        // to concepts/. v0.1 is exact-slug-AND-subdir.
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("wiki/entities/foo.md");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "x").unwrap();

        // Fact is concept, not entity → should resolve to concepts/foo.md (new)
        let r = decide(dir.path(), &make_fact(FactKind::Concept, "foo"));
        assert!(r.is_new());
        assert!(r.path().ends_with("wiki/concepts/foo.md"));
    }

    /// A page exists at slug `alluvium-design`. A new fact comes in with
    /// slug `alluvium-design-doc` — close enough that fuzzy should latch
    /// onto the existing page rather than creating a duplicate.
    #[test]
    fn fuzzy_slug_matches_existing_page() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("wiki/concepts/alluvium-design.md");
        std::fs::create_dir_all(existing.parent().unwrap()).unwrap();
        std::fs::write(
            &existing,
            "---\ntitle: Alluvium Design\n---\n\n# Alluvium Design\n",
        )
        .unwrap();

        let fact = make_fact(FactKind::Concept, "alluvium-design-doc");
        let r = decide(dir.path(), &fact);
        assert!(!r.is_new(), "fuzzy slug should latch onto existing page");
        assert_eq!(r.path(), existing);
    }

    /// Slugs differ entirely but the frontmatter title matches the new
    /// fact's page_title — fuzzy should still find it via the title axis.
    #[test]
    fn fuzzy_title_matches_when_slug_differs() {
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("wiki/concepts/old-slug.md");
        std::fs::create_dir_all(existing.parent().unwrap()).unwrap();
        std::fs::write(
            &existing,
            "---\ntitle: Hook Payload\n---\n\n# Hook Payload\n",
        )
        .unwrap();

        let mut fact = make_fact(FactKind::Concept, "hook-payload");
        fact.page_title = "Hook Payload".into();
        let r = decide(dir.path(), &fact);
        assert!(!r.is_new(), "title match should latch onto existing page");
        assert_eq!(r.path(), existing);
    }

    /// Unrelated existing page — fuzzy must NOT false-positive.
    #[test]
    fn unrelated_existing_does_not_fuzzy_match() {
        let dir = tempfile::tempdir().unwrap();
        let unrelated = dir.path().join("wiki/concepts/database-schemas.md");
        std::fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
        std::fs::write(
            &unrelated,
            "---\ntitle: Database Schemas\n---\n\n# Database Schemas\n",
        )
        .unwrap();

        let r = decide(dir.path(), &make_fact(FactKind::Concept, "alluvium-hooks"));
        assert!(r.is_new(), "unrelated topic should not fuzzy-match");
    }
}
