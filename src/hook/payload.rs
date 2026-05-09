//! Claude Code hook stdin payload.
//!
//! When a Claude Code Plugin hook fires, the command receives a JSON payload
//! on **stdin** (not via environment variables — `$SESSION_ID` does not exist).
//!
//! Schema (verified against Claude Code Hooks Reference):
//!
//! ```json
//! {
//!   "session_id": "abc123",
//!   "transcript_path": "/path/to/transcript.jsonl",
//!   "cwd": "/current/working/directory",
//!   "permission_mode": "default",
//!   "hook_event_name": "SessionStart" | "PreCompact" | "Stop" | "SessionEnd" | ...
//! }
//! ```
//!
//! See ADR-011 in `docs/DECISIONS.md`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookPayload {
    pub session_id: String,
    pub transcript_path: std::path::PathBuf,
    pub cwd: std::path::PathBuf,
    #[serde(default)]
    pub permission_mode: Option<String>,
    pub hook_event_name: String,
    /// Optional sub-agent fields (only present when running inside a sub-agent context).
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
}

/// Maximum bytes of stdin we'll consume. Safety limit: refuse pathological input
/// rather than allocate unbounded memory if Claude Code ever changes its
/// payload size semantics.
const MAX_PAYLOAD_BYTES: usize = 1024 * 1024; // 1 MiB

/// Read and parse the hook payload from stdin.
///
/// Returns an error if stdin can't be read, exceeds [`MAX_PAYLOAD_BYTES`], or
/// the bytes don't deserialize into a [`HookPayload`].
pub fn read_from_stdin() -> Result<HookPayload> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin()
        .take(MAX_PAYLOAD_BYTES as u64 + 1)
        .read_to_string(&mut buf)
        .context("failed to read hook payload from stdin")?;
    if buf.len() > MAX_PAYLOAD_BYTES {
        anyhow::bail!(
            "hook payload exceeds {} bytes (got {}); refusing to parse",
            MAX_PAYLOAD_BYTES,
            buf.len()
        );
    }
    parse(&buf)
}

/// Parse a JSON string into a [`HookPayload`].
///
/// Pulled out of [`read_from_stdin`] so unit tests can exercise it without
/// the global stdin handle.
pub fn parse(json: &str) -> Result<HookPayload> {
    serde_json::from_str(json).with_context(|| {
        let preview: String = json.chars().take(200).collect();
        format!("failed to parse hook payload JSON; first 200 chars: {preview}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_JSON: &str = r#"{
        "session_id": "abc123",
        "transcript_path": "/Users/eric/.claude/projects/x/y.jsonl",
        "cwd": "/Users/eric/work",
        "permission_mode": "default",
        "hook_event_name": "Stop",
        "agent_id": "agent-1",
        "agent_type": "general-purpose"
    }"#;

    const MINIMAL_JSON: &str = r#"{
        "session_id": "abc",
        "transcript_path": "/x.jsonl",
        "cwd": "/y",
        "hook_event_name": "Stop"
    }"#;

    #[test]
    fn full_payload_parses() {
        let p = parse(FULL_JSON).unwrap();
        assert_eq!(p.session_id, "abc123");
        assert_eq!(
            p.transcript_path.to_string_lossy(),
            "/Users/eric/.claude/projects/x/y.jsonl"
        );
        assert_eq!(p.cwd.to_string_lossy(), "/Users/eric/work");
        assert_eq!(p.permission_mode.as_deref(), Some("default"));
        assert_eq!(p.hook_event_name, "Stop");
        assert_eq!(p.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(p.agent_type.as_deref(), Some("general-purpose"));
    }

    #[test]
    fn minimal_payload_parses_with_defaults() {
        let p = parse(MINIMAL_JSON).unwrap();
        assert_eq!(p.session_id, "abc");
        assert_eq!(p.permission_mode, None);
        assert_eq!(p.agent_id, None);
        assert_eq!(p.agent_type, None);
    }

    #[test]
    fn missing_required_field_errors() {
        // session_id is required; serde will refuse this.
        let json = r#"{
            "transcript_path": "/x",
            "cwd": "/y",
            "hook_event_name": "Stop"
        }"#;
        let err = parse(json).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("session_id"),
            "expected error to name the missing field; got: {msg}"
        );
    }

    #[test]
    fn invalid_json_errors_with_context() {
        let err = parse("not even json").unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("failed to parse hook payload"),
            "expected our context wrapper; got: {msg}"
        );
    }

    #[test]
    fn extra_unknown_fields_are_ignored() {
        let json = r#"{
            "session_id": "x",
            "transcript_path": "/x",
            "cwd": "/y",
            "hook_event_name": "Stop",
            "future_field_we_dont_know": "value",
            "another_one": 42
        }"#;
        let p = parse(json).expect("forward-compat: unknown fields must be ignored");
        assert_eq!(p.session_id, "x");
    }

    #[test]
    fn error_preview_is_bounded() {
        // 10K of garbage; error message should not embed all of it.
        let huge: String = "X".repeat(10_000);
        let err = parse(&huge).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.len() < 2000,
            "error message should truncate input preview; got {} chars",
            msg.len()
        );
    }

    #[test]
    fn round_trip_serialize_deserialize() {
        let original = parse(FULL_JSON).unwrap();
        let json = serde_json::to_string(&original).unwrap();
        let round_tripped = parse(&json).unwrap();
        assert_eq!(original.session_id, round_tripped.session_id);
        assert_eq!(original.transcript_path, round_tripped.transcript_path);
        assert_eq!(original.cwd, round_tripped.cwd);
        assert_eq!(original.permission_mode, round_tripped.permission_mode);
        assert_eq!(original.hook_event_name, round_tripped.hook_event_name);
        assert_eq!(original.agent_id, round_tripped.agent_id);
        assert_eq!(original.agent_type, round_tripped.agent_type);
    }

    #[test]
    fn all_documented_event_names_parse() {
        for event in [
            "SessionStart",
            "PreCompact",
            "Stop",
            "SessionEnd",
            "UserPromptSubmit",
            "PostToolUse",
        ] {
            let json = format!(
                r#"{{"session_id":"x","transcript_path":"/x","cwd":"/y","hook_event_name":"{event}"}}"#
            );
            let p =
                parse(&json).unwrap_or_else(|e| panic!("event {event} should parse, got {e:?}"));
            assert_eq!(p.hook_event_name, event);
        }
    }
}
