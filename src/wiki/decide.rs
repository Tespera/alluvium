//! Decide whether a fact targets a new or existing topic page.
//!
//! Touches the filesystem once (`Path::exists`) to flip between
//! [`TargetPage::Existing`] and [`TargetPage::New`].

use std::path::Path;

use crate::extraction::ExtractedFact;

use super::TargetPage;

/// Compute the [`TargetPage`] for this fact under `alluvium_root`.
pub fn decide(alluvium_root: &Path, fact: &ExtractedFact) -> TargetPage {
    let path = super::locator::page_path(alluvium_root, fact);
    if path.exists() {
        TargetPage::Existing(path)
    } else {
        TargetPage::New(path)
    }
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
}
