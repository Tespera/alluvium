//! Compute the canonical filesystem path for a fact's topic page.
//!
//! Pure path arithmetic — never touches the filesystem. The new-vs-existing
//! decision lives in [`super::decide`].

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
}
