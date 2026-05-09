//! Parse Anthropic Messages API response into [`DistillerOutput`].
//!
//! Pure function: takes the JSON `Value` returned by
//! [`crate::distiller::client::AnthropicClient::messages`], extracts the
//! assistant's text content, locates the JSON object the LLM emitted, and
//! validates it against the v0.1 schema (ADR-013).
//!
//! ## Robustness
//!
//! Many of the things that can go wrong here are not bugs — they're the
//! reality of LLMs occasionally drifting from instructions. We handle:
//!
//! - **Code-fenced JSON** (```` ```json ... ``` ```` ): unwrap before parsing.
//! - **Prose surrounding JSON**: locate the outermost `{ ... }` and parse it.
//! - **Missing fields on individual facts**: log + drop that fact, continue
//!   with the rest. (ADR-013: per-fact errors are non-fatal.)
//! - **Schema-level errors** (no `extracted` array, top-level not an object,
//!   no text content at all): propagate as a hard error. The caller must
//!   bail and the archive fails.
//! - **Unknown keys**: silently ignored (forward-compat for future schema
//!   additions).

use anyhow::{Context, Result};

use super::{DistillerOutput, TokenUsage};
use crate::extraction::ExtractedFact;

/// Parse Anthropic's Messages API response JSON into our typed output.
pub fn parse(response: &serde_json::Value) -> Result<DistillerOutput> {
    let usage = response.get("usage").and_then(token_usage_from_value);

    let text = extract_assistant_text(response)
        .context("Anthropic response had no assistant text content block")?;
    let json_str = unwrap_code_fence(&text);
    let json_str =
        locate_json_object(json_str).context("could not locate a JSON object in LLM output")?;

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

/// Find the assistant's text content block in an Anthropic response.
/// Concatenates multiple text blocks (rare but possible) with `\n`.
fn extract_assistant_text(response: &serde_json::Value) -> Option<String> {
    let blocks = response.get("content")?.as_array()?;
    let texts: Vec<&str> = blocks
        .iter()
        .filter_map(|block| {
            if block.get("type")? == &serde_json::Value::String("text".into()) {
                block.get("text")?.as_str()
            } else {
                None
            }
        })
        .collect();
    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
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

/// Locate the outermost `{ ... }` JSON object in a string. Tolerates prose
/// before / after.
fn locate_json_object(s: &str) -> Option<&str> {
    let start = s.find('{')?;
    let end = s.rfind('}')?;
    if end > start {
        Some(&s[start..=end])
    } else {
        None
    }
}

fn token_usage_from_value(usage: &serde_json::Value) -> Option<TokenUsage> {
    let input = usage.get("input_tokens")?.as_u64()?;
    let output = usage.get("output_tokens")?.as_u64()?;
    Some(TokenUsage {
        input_tokens: input,
        output_tokens: output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::FactKind;

    fn full_response_json(content_text: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-haiku-4-5",
            "content": [{"type": "text", "text": content_text}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 100, "output_tokens": 50}
        })
    }

    fn good_distiller_output_json() -> &'static str {
        r#"{
            "title": "Designing Alluvium",
            "tags": ["alluvium", "claude-code", "rust"],
            "extracted": [
                {
                    "type": "concept",
                    "page_slug": "claude-code-hook-payload",
                    "page_title": "Claude Code Hook Payload",
                    "summary": "Hooks receive context via stdin JSON.",
                    "body_markdown": "When a hook fires, the command gets a JSON payload on stdin.",
                    "relations": {
                        "uses": [],
                        "used-by": ["alluvium"],
                        "related": ["claude-code-plugins"],
                        "supersedes": []
                    },
                    "confidence": 0.95
                },
                {
                    "type": "entity",
                    "page_slug": "alluvium",
                    "page_title": "Alluvium",
                    "summary": "Auto-archiver for Claude Code sessions.",
                    "body_markdown": "Alluvium is a Rust CLI that watches Claude Code sessions...",
                    "relations": {"uses": [], "used-by": [], "related": [], "supersedes": []},
                    "confidence": 1.0
                }
            ]
        }"#
    }

    // ─────────────── full happy path ───────────────

    #[test]
    fn parses_complete_response() {
        let response = full_response_json(good_distiller_output_json());
        let out = parse(&response).unwrap();
        assert_eq!(out.title, "Designing Alluvium");
        assert_eq!(out.tags, vec!["alluvium", "claude-code", "rust"]);
        assert_eq!(out.facts.len(), 2);
        assert_eq!(out.facts[0].kind, FactKind::Concept);
        assert_eq!(out.facts[0].page_slug, "claude-code-hook-payload");
        assert_eq!(out.facts[0].relations.used_by, vec!["alluvium"]);
        assert_eq!(out.facts[1].kind, FactKind::Entity);
    }

    #[test]
    fn usage_is_extracted() {
        let response = full_response_json(good_distiller_output_json());
        let out = parse(&response).unwrap();
        let u = out.usage.unwrap();
        assert_eq!(u.input_tokens, 100);
        assert_eq!(u.output_tokens, 50);
    }

    #[test]
    fn missing_usage_is_ok() {
        let response = serde_json::json!({
            "content": [{"type": "text", "text": good_distiller_output_json()}]
        });
        let out = parse(&response).unwrap();
        assert!(out.usage.is_none());
    }

    // ─────────────── code fence handling ───────────────

    #[test]
    fn code_fence_with_json_lang_is_unwrapped() {
        let fenced = format!("```json\n{}\n```", good_distiller_output_json());
        let response = full_response_json(&fenced);
        let out = parse(&response).unwrap();
        assert_eq!(out.facts.len(), 2);
    }

    #[test]
    fn code_fence_without_lang_is_unwrapped() {
        let fenced = format!("```\n{}\n```", good_distiller_output_json());
        let response = full_response_json(&fenced);
        let out = parse(&response).unwrap();
        assert_eq!(out.facts.len(), 2);
    }

    #[test]
    fn prose_around_json_is_tolerated() {
        let prosey = format!(
            "Sure! Here's the analysis:\n\n{}\n\nLet me know if you need anything else.",
            good_distiller_output_json()
        );
        let response = full_response_json(&prosey);
        let out = parse(&response).unwrap();
        assert_eq!(out.title, "Designing Alluvium");
    }

    // ─────────────── default-fill ───────────────

    #[test]
    fn missing_tags_defaults_to_empty() {
        let json = r#"{"title":"x","extracted":[]}"#;
        let response = full_response_json(json);
        let out = parse(&response).unwrap();
        assert!(out.tags.is_empty());
    }

    #[test]
    fn empty_extracted_array_yields_zero_facts() {
        let json = r#"{"title":"x","tags":[],"extracted":[]}"#;
        let response = full_response_json(json);
        let out = parse(&response).unwrap();
        assert_eq!(out.facts.len(), 0);
    }

    #[test]
    fn fact_missing_relations_defaults_to_empty() {
        let json = r#"{
            "title": "x",
            "extracted": [{
                "type": "concept",
                "page_slug": "a",
                "page_title": "A",
                "summary": "s",
                "body_markdown": "b"
            }]
        }"#;
        let response = full_response_json(json);
        let out = parse(&response).unwrap();
        assert_eq!(out.facts.len(), 1);
        assert!(out.facts[0].relations.is_empty());
        assert_eq!(out.facts[0].confidence, 1.0);
    }

    // ─────────────── per-fact error tolerance ───────────────

    #[test]
    fn one_malformed_fact_is_dropped_others_kept() {
        // First fact is missing required 'page_slug'. Second is fine.
        // Per ADR-013 we drop the bad one and keep the good one.
        let json = r#"{
            "title": "x",
            "extracted": [
                {"type": "concept", "page_title": "A", "summary": "s", "body_markdown": "b"},
                {"type": "concept", "page_slug": "good", "page_title": "Good", "summary": "s", "body_markdown": "b"}
            ]
        }"#;
        let response = full_response_json(json);
        let out = parse(&response).unwrap();
        assert_eq!(out.facts.len(), 1);
        assert_eq!(out.facts[0].page_slug, "good");
    }

    #[test]
    fn unknown_fact_type_is_treated_as_malformed() {
        let json = r#"{
            "title": "x",
            "extracted": [
                {"type": "future_kind", "page_slug": "a", "page_title": "A", "summary": "s", "body_markdown": "b"}
            ]
        }"#;
        let response = full_response_json(json);
        let out = parse(&response).unwrap();
        assert!(out.facts.is_empty());
    }

    // ─────────────── hard schema errors ───────────────

    #[test]
    fn missing_title_is_a_hard_error() {
        let json = r#"{"tags":[],"extracted":[]}"#;
        let response = full_response_json(json);
        let err = parse(&response).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("'title'"), "got: {msg}");
    }

    #[test]
    fn missing_extracted_is_a_hard_error() {
        let json = r#"{"title":"x","tags":[]}"#;
        let response = full_response_json(json);
        let err = parse(&response).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("'extracted'"), "got: {msg}");
    }

    #[test]
    fn no_text_content_is_a_hard_error() {
        let response = serde_json::json!({"content": []});
        let err = parse(&response).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("no assistant text"), "got: {msg}");
    }

    #[test]
    fn non_json_content_is_a_hard_error() {
        let response = full_response_json("this is not even close to JSON");
        let err = parse(&response).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("could not locate") || msg.contains("not valid JSON"),
            "got: {msg}"
        );
    }

    // ─────────────── all four FactKind values ───────────────

    #[test]
    fn parses_all_four_fact_kinds() {
        let json = r#"{
            "title": "x",
            "extracted": [
                {"type": "concept",  "page_slug": "c", "page_title": "C", "summary": "s", "body_markdown": "b"},
                {"type": "entity",   "page_slug": "e", "page_title": "E", "summary": "s", "body_markdown": "b"},
                {"type": "decision", "page_slug": "d", "page_title": "D", "summary": "s", "body_markdown": "b"},
                {"type": "gotcha",   "page_slug": "g", "page_title": "G", "summary": "s", "body_markdown": "b"}
            ]
        }"#;
        let response = full_response_json(json);
        let out = parse(&response).unwrap();
        assert_eq!(out.facts.len(), 4);
        let kinds: Vec<FactKind> = out.facts.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            vec![
                FactKind::Concept,
                FactKind::Entity,
                FactKind::Decision,
                FactKind::Gotcha
            ]
        );
    }

    // ─────────────── relations kebab-case bridging ───────────────

    #[test]
    fn used_by_field_is_kebab_case_in_input() {
        let json = r#"{
            "title": "x",
            "extracted": [{
                "type": "concept",
                "page_slug": "a",
                "page_title": "A",
                "summary": "s",
                "body_markdown": "b",
                "relations": {
                    "uses": ["other"],
                    "used-by": ["caller"],
                    "related": [],
                    "supersedes": []
                }
            }]
        }"#;
        let response = full_response_json(json);
        let out = parse(&response).unwrap();
        assert_eq!(out.facts[0].relations.uses, vec!["other"]);
        assert_eq!(out.facts[0].relations.used_by, vec!["caller"]);
    }

    // ─────────────── unknown extra fields ───────────────

    #[test]
    fn unknown_top_level_keys_are_ignored() {
        // Forward-compat: future schema additions should not break v0.1 parser.
        let json = r#"{
            "title": "x",
            "tags": [],
            "extracted": [],
            "future_field_we_have_not_seen": {"some": "thing"}
        }"#;
        let response = full_response_json(json);
        let out = parse(&response).unwrap();
        assert_eq!(out.title, "x");
    }

    #[test]
    fn unknown_fact_keys_are_ignored() {
        let json = r#"{
            "title": "x",
            "extracted": [{
                "type": "concept",
                "page_slug": "a",
                "page_title": "A",
                "summary": "s",
                "body_markdown": "b",
                "evidence": [{"from_message_index": 4, "snippet": "hi"}],
                "future_extra": 42
            }]
        }"#;
        let response = full_response_json(json);
        let out = parse(&response).unwrap();
        assert_eq!(out.facts.len(), 1);
        // evidence and future_extra are silently dropped (v0.1 schema).
    }
}
