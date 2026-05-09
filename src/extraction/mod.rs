//! Extracted-fact domain types.
//!
//! These are the typed atoms a distillation produces (per ADR-012). The
//! actual work of producing them happens in
//! [`crate::distiller::parser`] (parsing the LLM's JSON output) and is
//! orchestrated from `cli/archive.rs`.
//!
//! Reserved as a logical home for future post-processing (confidence
//! filtering, embedding-based dedup, cross-fact reconciliation), but in
//! v0.1 the module is just type definitions.

use serde::{Deserialize, Serialize};

/// One extracted knowledge atom from a Claude Code session.
///
/// See ADR-012 for the design rationale and ADR-013 for the JSON schema
/// the LLM is asked to produce.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedFact {
    /// What kind of thing this is. Drives `wiki/concepts/` vs `wiki/entities/`.
    #[serde(rename = "type")]
    pub kind: FactKind,
    /// Canonical kebab-case slug — dedup key for matching topic pages.
    pub page_slug: String,
    /// Human-readable title for the topic page.
    pub page_title: String,
    /// 1-3 sentence core claim. Goes into `log.md` and feeds the fact_id
    /// hash (ADR-014).
    pub summary: String,
    /// Freeform 2-6 paragraph markdown that goes inside the
    /// HTML-comment-bracketed block on the topic page (ADR-009).
    pub body_markdown: String,
    /// Typed wikilink relations.
    #[serde(default)]
    pub relations: Relations,
    /// LLM self-reported confidence (0.0-1.0). Below 0.5 logs a warning
    /// but the fact is still kept (caller decides whether to act).
    #[serde(default = "default_confidence")]
    pub confidence: f32,
}

fn default_confidence() -> f32 {
    1.0
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FactKind {
    Entity,
    Concept,
    Decision,
    Gotcha,
}

impl FactKind {
    /// Subdirectory under `wiki/` where this kind's primary page lives.
    /// `decision` and `gotcha` attach to `concepts` since they are
    /// usually distilled patterns.
    pub fn wiki_subdir(self) -> &'static str {
        match self {
            FactKind::Entity => "entities",
            FactKind::Concept | FactKind::Decision | FactKind::Gotcha => "concepts",
        }
    }
}

/// Typed wikilink relations. JSON keys are kebab-case (matches frontmatter
/// convention); Rust fields are snake_case via serde rename.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub struct Relations {
    #[serde(default)]
    pub uses: Vec<String>,
    #[serde(default)]
    pub used_by: Vec<String>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub supersedes: Vec<String>,
}

impl Relations {
    pub fn is_empty(&self) -> bool {
        self.uses.is_empty()
            && self.used_by.is_empty()
            && self.related.is_empty()
            && self.supersedes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fact_kind_serializes_lowercase() {
        let json = serde_json::to_string(&FactKind::Concept).unwrap();
        assert_eq!(json, r#""concept""#);
    }

    #[test]
    fn fact_kind_deserializes_lowercase() {
        let kind: FactKind = serde_json::from_str(r#""entity""#).unwrap();
        assert_eq!(kind, FactKind::Entity);
    }

    #[test]
    fn relations_used_by_uses_kebab_case_in_json() {
        let r = Relations {
            uses: vec!["a".into()],
            used_by: vec!["b".into()],
            related: vec![],
            supersedes: vec![],
        };
        let json = serde_json::to_value(&r).unwrap();
        assert!(
            json.get("used-by").is_some(),
            "should serialize as 'used-by'"
        );
        assert!(json.get("used_by").is_none(), "should not use 'used_by'");
    }

    #[test]
    fn relations_round_trip_through_kebab_case() {
        let json = serde_json::json!({
            "uses": ["x"],
            "used-by": ["y"],
            "related": [],
            "supersedes": []
        });
        let r: Relations = serde_json::from_value(json).unwrap();
        assert_eq!(r.uses, vec!["x"]);
        assert_eq!(r.used_by, vec!["y"]);
    }

    #[test]
    fn confidence_defaults_to_one_when_missing() {
        let json = serde_json::json!({
            "type": "concept",
            "page_slug": "x",
            "page_title": "X",
            "summary": "s",
            "body_markdown": "b"
        });
        let f: ExtractedFact = serde_json::from_value(json).unwrap();
        assert_eq!(f.confidence, 1.0);
    }

    #[test]
    fn relations_default_is_empty() {
        let json = serde_json::json!({
            "type": "concept",
            "page_slug": "x",
            "page_title": "X",
            "summary": "s",
            "body_markdown": "b"
        });
        let f: ExtractedFact = serde_json::from_value(json).unwrap();
        assert!(f.relations.is_empty());
    }

    #[test]
    fn wiki_subdir_routing() {
        assert_eq!(FactKind::Entity.wiki_subdir(), "entities");
        assert_eq!(FactKind::Concept.wiki_subdir(), "concepts");
        assert_eq!(FactKind::Decision.wiki_subdir(), "concepts");
        assert_eq!(FactKind::Gotcha.wiki_subdir(), "concepts");
    }
}
