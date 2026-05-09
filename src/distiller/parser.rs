//! Parse the LLM's text output into [`DistillerOutput`].
//!
//! Backend-agnostic: takes the raw `text` returned by an [`LlmBackend`]
//! (whether `claude -p`, Anthropic API, OpenAI, DeepSeek, or Gemini) and
//! validates it against the v0.1 schema (ADR-013).
//!
//! ## Robustness (per ADR-013)
//!
//! - **Code-fenced JSON** (`json`/plain ```` ``` ````): unwrap before parsing.
//! - **Prose around JSON**: locate the outermost `{ ... }` and parse that.
//! - **Per-fact errors**: drop bad ones, keep good ones, continue.
//! - **Hard errors** (no `extracted` array, no JSON object found, malformed
//!   JSON): propagate so the caller fails the archive cleanly.
//! - **Unknown keys**: silently ignored (forward-compat).

use anyhow::{Context, Result};

use super::DistillerOutput;
use crate::extraction::ExtractedFact;

/// Parse the model's output text into our typed output. The optional
/// `usage` is plumbed through from the backend (when available).
pub fn parse(text: &str, usage: Option<super::TokenUsage>) -> Result<DistillerOutput> {
    let unfenced = unwrap_code_fence(text);
    let json_str =
        locate_json_object(unfenced).context("could not locate a JSON object in LLM output")?;

    let value: serde_json::Value = serde_json::from_str(json_str).with_context(|| {
        let preview: String = json_str.chars().take(500).collect();
        format!("LLM output was not valid JSON; first 500 chars: {preview}")
    })?;

    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .context("missing required 'title' field in distiller output")?
        .to_string();

    let tags: Vec<String> = value
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let raw_facts = value
        .get("extracted")
        .and_then(|v| v.as_array())
        .context("missing required 'extracted' array in distiller output")?;

    let mut facts = Vec::with_capacity(raw_facts.len());
    for (i, raw) in raw_facts.iter().enumerate() {
        match serde_json::from_value::<ExtractedFact>(raw.clone()) {
            Ok(fact) => {
                if fact.confidence < 0.5 {
                    tracing::warn!(
                        slug = %fact.page_slug,
                        confidence = fact.confidence,
                        "low-confidence fact, kept anyway"
                    );
                }
                facts.push(fact);
            }
            Err(err) => {
                tracing::warn!(
                    fact_index = i,
                    error = %err,
                    "skipping malformed fact in distiller output"
                );
            }
        }
    }

    Ok(DistillerOutput {
        title,
        tags,
        facts,
        usage,
    })
}

/// Strip ``` ```json … ``` ``` or ``` ``` … ``` ``` wrapping if present.
fn unwrap_code_fence(s: &str) -> &str {
    let trimmed = s.trim();
    let inner = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|rest| rest.trim_start_matches('\n'))
        .map(|rest| rest.trim_start());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::FactKind;

    fn good_json() -> &'static str {
        r#"{
            "title": "Designing Alluvium",
            "tags": ["alluvium", "claude-code"],
            "extracted": [
                {
                    "type": "concept",
                    "page_slug": "claude-code-hook-payload",
                    "page_title": "Claude Code Hook Payload",
                    "summary": "Hooks receive context via stdin JSON.",
                    "body_markdown": "When a hook fires, the command gets a JSON payload on stdin.",
                    "relations": {"uses":[],"used-by":["alluvium"],"related":[],"supersedes":[]},
                    "confidence": 0.95
                },
                {
                    "type": "entity",
                    "page_slug": "alluvium",
                    "page_title": "Alluvium",
                    "summary": "Auto-archiver.",
                    "body_markdown": "Alluvium is a Rust CLI...",
                    "relations": {"uses":[],"used-by":[],"related":[],"supersedes":[]},
                    "confidence": 1.0
                }
            ]
        }"#
    }

    #[test]
    fn parses_complete_response() {
        let out = parse(good_json(), None).unwrap();
        assert_eq!(out.title, "Designing Alluvium");
        assert_eq!(out.tags, vec!["alluvium", "claude-code"]);
        assert_eq!(out.facts.len(), 2);
        assert_eq!(out.facts[0].kind, FactKind::Concept);
        assert_eq!(out.facts[1].kind, FactKind::Entity);
    }

    #[test]
    fn usage_is_passed_through_from_backend() {
        let usage = super::super::TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
        };
        let out = parse(good_json(), Some(usage)).unwrap();
        let u = out.usage.unwrap();
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 50);
    }

    #[test]
    fn code_fenced_json_is_unwrapped() {
        let fenced = format!("```json\n{}\n```", good_json());
        let out = parse(&fenced, None).unwrap();
        assert_eq!(out.facts.len(), 2);
    }

    #[test]
    fn fence_without_lang_unwrapped() {
        let fenced = format!("```\n{}\n```", good_json());
        let out = parse(&fenced, None).unwrap();
        assert_eq!(out.facts.len(), 2);
    }

    #[test]
    fn prose_around_json_tolerated() {
        let prosey = format!(
            "Here's the analysis:\n\n{}\n\nHope this helps!",
            good_json()
        );
        let out = parse(&prosey, None).unwrap();
        assert_eq!(out.title, "Designing Alluvium");
    }

    #[test]
    fn missing_tags_defaults_to_empty() {
        let out = parse(r#"{"title":"x","extracted":[]}"#, None).unwrap();
        assert!(out.tags.is_empty());
    }

    #[test]
    fn empty_extracted_array_yields_zero_facts() {
        let out = parse(r#"{"title":"x","tags":[],"extracted":[]}"#, None).unwrap();
        assert_eq!(out.facts.len(), 0);
    }

    #[test]
    fn malformed_fact_dropped_others_kept() {
        let json = r#"{
            "title": "x",
            "extracted": [
                {"type": "concept", "page_title": "A", "summary": "s", "body_markdown": "b"},
                {"type": "concept", "page_slug": "good", "page_title": "Good", "summary": "s", "body_markdown": "b"}
            ]
        }"#;
        let out = parse(json, None).unwrap();
        assert_eq!(out.facts.len(), 1);
        assert_eq!(out.facts[0].page_slug, "good");
    }

    #[test]
    fn unknown_fact_type_dropped() {
        let json = r#"{"title":"x","extracted":[{"type":"future","page_slug":"a","page_title":"A","summary":"s","body_markdown":"b"}]}"#;
        let out = parse(json, None).unwrap();
        assert!(out.facts.is_empty());
    }

    #[test]
    fn missing_title_hard_error() {
        let err = parse(r#"{"extracted":[]}"#, None).unwrap_err();
        assert!(format!("{err:#}").contains("'title'"));
    }

    #[test]
    fn missing_extracted_hard_error() {
        let err = parse(r#"{"title":"x"}"#, None).unwrap_err();
        assert!(format!("{err:#}").contains("'extracted'"));
    }

    #[test]
    fn non_json_hard_error() {
        let err = parse("this is not even close to JSON", None).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("could not locate") || msg.contains("not valid JSON"));
    }

    #[test]
    fn parses_all_four_kinds() {
        let json = r#"{"title":"x","extracted":[
            {"type":"concept","page_slug":"c","page_title":"C","summary":"s","body_markdown":"b"},
            {"type":"entity","page_slug":"e","page_title":"E","summary":"s","body_markdown":"b"},
            {"type":"decision","page_slug":"d","page_title":"D","summary":"s","body_markdown":"b"},
            {"type":"gotcha","page_slug":"g","page_title":"G","summary":"s","body_markdown":"b"}
        ]}"#;
        let out = parse(json, None).unwrap();
        assert_eq!(out.facts.len(), 4);
    }

    #[test]
    fn unknown_top_level_keys_ignored() {
        let json = r#"{"title":"x","tags":[],"extracted":[],"future_key":"value"}"#;
        let out = parse(json, None).unwrap();
        assert_eq!(out.title, "x");
    }
}
