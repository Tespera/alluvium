//! Extract session-level metadata from a parsed transcript.
//!
//! Reduces a `&[Event]` (from [`crate::transcript::jsonl::read`]) into a
//! single `SessionMetadata` with:
//!
//! - `session_id`             — required; pulled from the first event that has one
//! - `cwd`                    — first envelope cwd we encounter (bookkeeping events skip it)
//! - `started_at` / `ended_at` — min/max timestamp across the whole stream
//! - `model`                  — most recent **non-synthetic** assistant model
//! - `input_tokens` / `output_tokens` — summed across all assistant `usage` blocks
//! - `event_count`            — raw event count (downstream can filter)
//!
//! Robust to schema gaps: every field except `session_id` and `event_count`
//! is optional or defaults to 0.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::transcript::jsonl::Event;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    pub cwd: Option<PathBuf>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub model: Option<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub event_count: usize,
}

/// Sentinel model string Claude Code uses for synthetic API-error events.
/// We treat it as "no real model used here" when picking a representative model.
const SYNTHETIC_MODEL: &str = "<synthetic>";

pub fn extract(events: &[Event]) -> Result<SessionMetadata> {
    if events.is_empty() {
        anyhow::bail!("transcript has no events; cannot extract metadata");
    }

    let session_id = events
        .iter()
        .find_map(event_session_id)
        .ok_or_else(|| anyhow::anyhow!("no event in transcript has a sessionId"))?
        .to_string();

    let cwd = events.iter().find_map(event_cwd).map(PathBuf::from);

    let mut started_at: Option<DateTime<Utc>> = None;
    let mut ended_at: Option<DateTime<Utc>> = None;
    for event in events {
        if let Some(ts_str) = event_timestamp(event) {
            match parse_iso(ts_str) {
                Ok(ts) => {
                    started_at = Some(started_at.map_or(ts, |s| s.min(ts)));
                    ended_at = Some(ended_at.map_or(ts, |e| e.max(ts)));
                }
                Err(err) => {
                    tracing::warn!(
                        timestamp = ts_str,
                        error = %err,
                        "skipping event with unparsable timestamp",
                    );
                }
            }
        }
    }

    // Most recent assistant event with a real model name.
    let model = events.iter().rev().find_map(|e| match e {
        Event::Assistant(c) => match c.message.model.as_deref() {
            Some(m) if m != SYNTHETIC_MODEL => Some(m.to_string()),
            _ => None,
        },
        _ => None,
    });

    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    for event in events {
        if let Event::Assistant(c) = event {
            if let Some(usage) = &c.message.usage {
                input_tokens = input_tokens.saturating_add(
                    usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                );
                output_tokens = output_tokens.saturating_add(
                    usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0),
                );
            }
        }
    }

    Ok(SessionMetadata {
        session_id,
        cwd,
        started_at,
        ended_at,
        model,
        input_tokens,
        output_tokens,
        event_count: events.len(),
    })
}

fn event_session_id(event: &Event) -> Option<&str> {
    match event {
        Event::User(c) | Event::Assistant(c) => Some(&c.envelope.session_id),
        Event::System(s) => Some(&s.envelope.session_id),
        Event::Attachment(a) => Some(&a.envelope.session_id),
        Event::FileHistorySnapshot(fhs) => fhs.session_id.as_deref(),
        Event::LastPrompt(lp) => Some(&lp.session_id),
        Event::PermissionMode(pm) => Some(&pm.session_id),
        Event::PrLink(pr) => Some(&pr.session_id),
        Event::QueueOperation(qo) => Some(&qo.session_id),
    }
}

fn event_cwd(event: &Event) -> Option<&str> {
    match event {
        Event::User(c) | Event::Assistant(c) => c.envelope.cwd.as_deref(),
        Event::System(s) => s.envelope.cwd.as_deref(),
        Event::Attachment(a) => a.envelope.cwd.as_deref(),
        // Bookkeeping events lack envelope.
        _ => None,
    }
}

fn event_timestamp(event: &Event) -> Option<&str> {
    match event {
        Event::User(c) | Event::Assistant(c) => c.envelope.timestamp.as_deref(),
        Event::System(s) => s.envelope.timestamp.as_deref(),
        Event::Attachment(a) => a.envelope.timestamp.as_deref(),
        Event::FileHistorySnapshot(fhs) => fhs.timestamp.as_deref(),
        Event::LastPrompt(lp) => lp.timestamp.as_deref(),
        Event::PermissionMode(pm) => pm.timestamp.as_deref(),
        Event::PrLink(pr) => pr.timestamp.as_deref(),
        Event::QueueOperation(qo) => qo.timestamp.as_deref(),
    }
}

fn parse_iso(s: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .with_context(|| format!("invalid ISO 8601 timestamp: {s}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evt(json: &str) -> Event {
        serde_json::from_str(json).expect("test event JSON should parse")
    }

    #[test]
    fn extracts_session_id_from_envelope() {
        let events = vec![evt(
            r#"{"type":"user","sessionId":"S1","message":{"role":"user","content":"hi"}}"#,
        )];
        let m = extract(&events).unwrap();
        assert_eq!(m.session_id, "S1");
    }

    #[test]
    fn session_id_extracted_even_from_bookkeeping_only_transcript() {
        let events = vec![evt(
            r#"{"type":"last-prompt","lastPrompt":"hi","sessionId":"S1"}"#,
        )];
        let m = extract(&events).unwrap();
        assert_eq!(m.session_id, "S1");
        assert!(m.cwd.is_none());
        assert!(m.started_at.is_none());
        assert!(m.model.is_none());
    }

    #[test]
    fn cwd_extracted_from_first_event_with_one() {
        let events = vec![
            evt(r#"{"type":"last-prompt","lastPrompt":"x","sessionId":"S1"}"#),
            evt(
                r#"{"type":"user","sessionId":"S1","cwd":"/Users/eric","message":{"role":"user","content":"hi"}}"#,
            ),
        ];
        let m = extract(&events).unwrap();
        assert_eq!(m.cwd.as_deref(), Some(std::path::Path::new("/Users/eric")));
    }

    #[test]
    fn cwd_none_when_no_event_has_one() {
        let events = vec![evt(
            r#"{"type":"last-prompt","lastPrompt":"x","sessionId":"S1"}"#,
        )];
        let m = extract(&events).unwrap();
        assert!(m.cwd.is_none());
    }

    #[test]
    fn started_and_ended_at_are_min_and_max_timestamps() {
        let events = vec![
            // Out-of-order on purpose to verify min/max not first/last.
            evt(
                r#"{"type":"user","sessionId":"S","timestamp":"2026-04-23T16:31:00Z","message":{"role":"user","content":"a"}}"#,
            ),
            evt(
                r#"{"type":"assistant","sessionId":"S","timestamp":"2026-04-23T16:32:00Z","message":{"role":"assistant","content":[{"type":"text","text":"b"}]}}"#,
            ),
            evt(
                r#"{"type":"user","sessionId":"S","timestamp":"2026-04-23T16:30:00Z","message":{"role":"user","content":"earlier"}}"#,
            ),
        ];
        let m = extract(&events).unwrap();
        assert_eq!(
            m.started_at.unwrap().to_rfc3339(),
            "2026-04-23T16:30:00+00:00"
        );
        assert_eq!(
            m.ended_at.unwrap().to_rfc3339(),
            "2026-04-23T16:32:00+00:00"
        );
    }

    #[test]
    fn timestamps_optional_when_no_event_has_them() {
        let events = vec![evt(
            r#"{"type":"last-prompt","lastPrompt":"x","sessionId":"S"}"#,
        )];
        let m = extract(&events).unwrap();
        assert!(m.started_at.is_none());
        assert!(m.ended_at.is_none());
    }

    #[test]
    fn invalid_timestamp_is_skipped_not_panic() {
        let events = vec![
            evt(
                r#"{"type":"user","sessionId":"S","timestamp":"NOT-A-DATE","message":{"role":"user","content":"x"}}"#,
            ),
            evt(
                r#"{"type":"user","sessionId":"S","timestamp":"2026-04-23T16:31:00Z","message":{"role":"user","content":"y"}}"#,
            ),
        ];
        let m = extract(&events).unwrap();
        assert_eq!(
            m.started_at.unwrap().to_rfc3339(),
            "2026-04-23T16:31:00+00:00"
        );
    }

    #[test]
    fn synthetic_model_skipped_real_model_used() {
        // Order: real model first, then synthetic API error second.
        // Iteration is reversed so the most-recent non-synthetic wins.
        let events = vec![
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","model":"claude-opus-4-7","content":[{"type":"text","text":"a"}]}}"#,
            ),
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"err"}]}}"#,
            ),
        ];
        let m = extract(&events).unwrap();
        assert_eq!(m.model.as_deref(), Some("claude-opus-4-7"));
    }

    #[test]
    fn most_recent_real_model_wins_when_session_switches_models() {
        let events = vec![
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","model":"claude-haiku-4-5","content":[{"type":"text","text":"a"}]}}"#,
            ),
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","model":"claude-opus-4-7","content":[{"type":"text","text":"b"}]}}"#,
            ),
        ];
        let m = extract(&events).unwrap();
        assert_eq!(m.model.as_deref(), Some("claude-opus-4-7"));
    }

    #[test]
    fn no_assistant_events_means_no_model() {
        let events = vec![evt(
            r#"{"type":"user","sessionId":"S","message":{"role":"user","content":"hi"}}"#,
        )];
        let m = extract(&events).unwrap();
        assert!(m.model.is_none());
    }

    #[test]
    fn token_sums_across_multiple_assistant_events() {
        let events = vec![
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"text","text":"a"}],"usage":{"input_tokens":100,"output_tokens":50}}}"#,
            ),
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"text","text":"b"}],"usage":{"input_tokens":200,"output_tokens":80}}}"#,
            ),
        ];
        let m = extract(&events).unwrap();
        assert_eq!(m.input_tokens, 300);
        assert_eq!(m.output_tokens, 130);
    }

    #[test]
    fn missing_usage_defaults_to_zero() {
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"text","text":"a"}]}}"#,
        )];
        let m = extract(&events).unwrap();
        assert_eq!(m.input_tokens, 0);
        assert_eq!(m.output_tokens, 0);
    }

    #[test]
    fn partial_usage_fields_handled() {
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"text","text":"a"}],"usage":{"input_tokens":42}}}"#,
        )];
        let m = extract(&events).unwrap();
        assert_eq!(m.input_tokens, 42);
        assert_eq!(m.output_tokens, 0);
    }

    #[test]
    fn empty_events_errors_with_helpful_message() {
        let events: Vec<Event> = vec![];
        let err = extract(&events).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("no events"), "got: {msg}");
    }

    #[test]
    fn event_count_matches_input_length() {
        let events = vec![
            evt(r#"{"type":"user","sessionId":"S","message":{"role":"user","content":"a"}}"#),
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"text","text":"b"}]}}"#,
            ),
            evt(r#"{"type":"system","sessionId":"S","subtype":"turn_duration","durationMs":42}"#),
        ];
        let m = extract(&events).unwrap();
        assert_eq!(m.event_count, 3);
    }

    #[test]
    fn iso_timestamp_with_milliseconds_parses() {
        // Real on-disk transcripts use millis + Z, e.g. "2026-04-23T16:31:44.393Z".
        let events = vec![evt(
            r#"{"type":"user","sessionId":"S","timestamp":"2026-04-23T16:31:44.393Z","message":{"role":"user","content":"hi"}}"#,
        )];
        let m = extract(&events).unwrap();
        let started = m.started_at.unwrap();
        assert_eq!(started.to_rfc3339(), "2026-04-23T16:31:44.393+00:00");
    }
}
