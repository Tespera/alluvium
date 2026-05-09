//! Read Claude Code session transcript JSONL files.
//!
//! ## Storage layout
//!
//! `~/.claude/projects/<encoded-cwd>/<sessionId>.jsonl` where the encoded cwd
//! has `/` and `.` replaced by `-` (spaces collapse to `-` too). The filename
//! (sans `.jsonl`) is the UUID `sessionId`; `sessionId` ALSO appears as a
//! field on virtually every line, so either works as ground truth.
//!
//! ## Schema notes (verified against on-disk files, Claude Code 2.1.x)
//!
//! - **9 top-level `type` values**: `user`, `assistant`, `system`,
//!   `attachment`, `file-history-snapshot`, `last-prompt`, `permission-mode`,
//!   `pr-link`, `queue-operation`.
//! - **`message.content` is polymorphic**: bare `String` for user-typed
//!   prompts, or an array of typed blocks (`text` / `thinking` / `tool_use` /
//!   `tool_result`). Kept as `serde_json::Value` here; downstream
//!   ([`crate::transcript::reconstruct`]) interprets it.
//! - **API errors come in two shapes**: synthetic-assistant (an `assistant`
//!   event with `message.model = "<synthetic>"` and `isApiErrorMessage`),
//!   and `system` events with `subtype: "api_error"` carrying `cause` /
//!   `retryAttempt` / `retryInMs`.
//! - **Compact boundaries** are `system` events with
//!   `subtype: "compact_boundary"`, carrying `compactMetadata` and
//!   `logicalParentUuid` instead of `parentUuid`.
//! - **Sub-agent (Task) calls** are inline in the same file, flagged by
//!   `isSidechain: true`, threaded by `parentUuid`.
//! - **Bookkeeping events** (`last-prompt`, `permission-mode`,
//!   `file-history-snapshot`, etc.) skip most envelope fields. Every
//!   envelope field except `sessionId` is `Option<...>`.
//! - **Schema drift**: `usage` block has gained fields across Claude Code
//!   versions; we keep it as `Value` so old transcripts still parse.
//!
//! ## Robustness
//!
//! [`read`] tolerates malformed individual lines: log via `tracing::warn!`
//! and skip rather than failing the whole file. A single corrupted line
//! must never block an archive.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Event {
    User(ConvEvent),
    Assistant(ConvEvent),
    System(SystemEvent),
    Attachment(AttachmentEvent),
    FileHistorySnapshot(FileHistorySnapshot),
    LastPrompt(LastPrompt),
    PermissionMode(PermissionMode),
    PrLink(PrLink),
    QueueOperation(QueueOperation),
}

impl Event {
    /// Per-event UUID. `None` for bookkeeping events that don't carry one.
    pub fn uuid(&self) -> Option<&str> {
        match self {
            Event::User(c) | Event::Assistant(c) => c.envelope.uuid.as_deref(),
            Event::System(s) => s.envelope.uuid.as_deref(),
            Event::Attachment(a) => a.envelope.uuid.as_deref(),
            Event::FileHistorySnapshot(_)
            | Event::LastPrompt(_)
            | Event::PermissionMode(_)
            | Event::PrLink(_)
            | Event::QueueOperation(_) => None,
        }
    }

    /// Session id, present on virtually every event.
    pub fn session_id(&self) -> Option<&str> {
        match self {
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

    /// Working directory associated with the event. Bookkeeping events lack envelope.
    pub fn cwd(&self) -> Option<&str> {
        match self {
            Event::User(c) | Event::Assistant(c) => c.envelope.cwd.as_deref(),
            Event::System(s) => s.envelope.cwd.as_deref(),
            Event::Attachment(a) => a.envelope.cwd.as_deref(),
            _ => None,
        }
    }

    /// ISO 8601 timestamp string. `None` if the event lacks one.
    pub fn timestamp(&self) -> Option<&str> {
        match self {
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

    /// True for sub-agent (sidechain) thread events. False for events that
    /// don't carry the flag (bookkeeping types).
    pub fn is_sidechain(&self) -> bool {
        match self {
            Event::User(c) | Event::Assistant(c) => c.envelope.is_sidechain,
            Event::System(s) => s.envelope.is_sidechain,
            Event::Attachment(a) => a.envelope.is_sidechain,
            _ => false,
        }
    }
}

/// Envelope fields shared by most top-level events.
///
/// Every field except `session_id` is optional because bookkeeping events
/// (and pre-version-x sessions) routinely omit subsets.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub session_id: String,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub logical_parent_uuid: Option<String>,
    pub timestamp: Option<String>,
    pub version: Option<String>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub user_type: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub is_sidechain: bool,
    #[serde(default)]
    pub is_meta: bool,
}

/// `user` and `assistant` events share the same schema (only the embedded
/// `message.role` differs).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvEvent {
    #[serde(flatten)]
    pub envelope: Envelope,
    pub message: RawMessage,
    pub prompt_id: Option<String>,
    pub request_id: Option<String>,
    pub permission_mode: Option<String>,
    pub source_tool_use_id: Option<String>,
    pub source_tool_assistant_uuid: Option<String>,
    /// Tool execution result, shape varies per tool. Kept loose.
    pub tool_use_result: Option<Value>,
    #[serde(default)]
    pub is_api_error_message: bool,
    pub api_error_status: Option<u16>,
    pub error: Option<Value>,
    pub image_paste_ids: Option<Vec<String>>,
    pub origin: Option<String>,
}

/// The `message` object embedded inside `user` / `assistant` events.
///
/// Named `RawMessage` (not `Message`) to avoid clashing with the higher-level
/// [`crate::transcript::Message`] that downstream `reconstruct` produces.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawMessage {
    pub role: String,
    /// Polymorphic. May be a bare `String` (typed user prompt) or an array
    /// of `ContentBlock`s. Downstream is responsible for the discrimination.
    pub content: Value,
    pub id: Option<String>,
    /// `"<synthetic>"` for assistant events that carry an inline API error.
    pub model: Option<String>,
    pub stop_reason: Option<String>,
    pub usage: Option<Value>,
    /// e.g. `"message"`, sometimes present sometimes not.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// One `content` array element when `RawMessage::content` is an array.
///
/// Provided as a convenience type for downstream code; not part of the
/// top-level enum since `content` is polymorphic.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        #[serde(default)]
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
        #[serde(default)]
        caller: Option<String>,
    },
    ToolResult {
        tool_use_id: String,
        content: Value,
        #[serde(default)]
        is_error: Option<bool>,
    },
}

/// `system` events. `subtype` is left as a free-form `String` because new
/// values appear across versions; downstream can match the strings it cares
/// about and fall through unknowns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemEvent {
    #[serde(flatten)]
    pub envelope: Envelope,
    /// Observed: `stop_hook_summary`, `turn_duration`, `local_command`,
    /// `compact_boundary`, `away_summary`, `api_error`, `informational`.
    pub subtype: String,
    pub content: Option<String>,
    pub level: Option<String>,
    pub duration_ms: Option<u64>,
    pub message_count: Option<u64>,
    pub hook_count: Option<u64>,
    pub hook_infos: Option<Value>,
    pub hook_errors: Option<Value>,
    pub prevented_continuation: Option<bool>,
    pub stop_reason: Option<String>,
    pub has_output: Option<bool>,
    pub tool_use_id: Option<String>,
    /// Only present on `subtype: "compact_boundary"`.
    pub compact_metadata: Option<Value>,
    /// Only present on `subtype: "api_error"`.
    pub cause: Option<Value>,
    pub retry_attempt: Option<u32>,
    pub retry_in_ms: Option<f64>,
    pub max_retries: Option<u32>,
    /// Cosmetic session name occasionally surfaced.
    pub slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentEvent {
    #[serde(flatten)]
    pub envelope: Envelope,
    pub attachment: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistorySnapshot {
    pub message_id: String,
    pub snapshot: Value,
    #[serde(default)]
    pub is_snapshot_update: bool,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastPrompt {
    pub last_prompt: String,
    pub session_id: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionMode {
    pub permission_mode: String,
    pub session_id: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrLink {
    pub session_id: String,
    pub pr_number: u64,
    pub pr_url: String,
    pub pr_repository: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueOperation {
    pub operation: String,
    pub content: Option<String>,
    pub session_id: String,
    pub timestamp: Option<String>,
}

/// Read a transcript JSONL file, returning all successfully-parsed events.
///
/// Malformed lines are logged at `warn` level (via `tracing`) and skipped;
/// a single bad line should never break the archive of an otherwise valid
/// session. Empty lines are silently ignored.
pub fn read(path: &Path) -> Result<Vec<Event>> {
    use std::fs::File;
    use std::io::{BufRead, BufReader};

    let file = File::open(path)
        .with_context(|| format!("failed to open transcript: {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    for (idx, line) in reader.lines().enumerate() {
        let line_no = idx + 1;
        let line =
            line.with_context(|| format!("read error at line {} of {}", line_no, path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<Event>(&line) {
            Ok(event) => events.push(event),
            Err(err) => {
                tracing::warn!(
                    line = line_no,
                    file = %path.display(),
                    error = %err,
                    "transcript line failed to parse, skipping",
                );
            }
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Helper: write `lines` (joined with newline) to a temp file, return its path.
    fn write_jsonl(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut tmp = tempfile::Builder::new()
            .suffix(".jsonl")
            .tempfile()
            .unwrap();
        for line in lines {
            writeln!(tmp, "{line}").unwrap();
        }
        tmp.flush().unwrap();
        tmp
    }

    fn parse_one<T: for<'de> Deserialize<'de>>(json: &str) -> T {
        serde_json::from_str(json).unwrap_or_else(|e| panic!("parse failed: {e}\ninput: {json}"))
    }

    #[test]
    fn user_event_string_content_parses() {
        let json = r#"{
            "type":"user",
            "sessionId":"s1",
            "uuid":"u1",
            "timestamp":"2026-04-23T16:31:44.393Z",
            "version":"2.1.118",
            "cwd":"/Users/eric/work",
            "userType":"human",
            "isSidechain":false,
            "message":{"role":"user","content":"Hello"}
        }"#;
        let event: Event = parse_one(json);
        let Event::User(conv) = event else {
            panic!("expected User variant");
        };
        assert_eq!(conv.envelope.session_id, "s1");
        assert_eq!(conv.envelope.uuid.as_deref(), Some("u1"));
        assert!(!conv.envelope.is_sidechain);
        assert_eq!(conv.message.role, "user");
        assert_eq!(conv.message.content, Value::String("Hello".into()));
    }

    #[test]
    fn assistant_event_array_content_parses() {
        let json = r#"{
            "type":"assistant",
            "sessionId":"s1",
            "uuid":"u2",
            "parentUuid":"u1",
            "isSidechain":false,
            "message":{
                "role":"assistant",
                "model":"claude-opus-4-7",
                "stopReason":"end_turn",
                "content":[
                    {"type":"thinking","thinking":"Let me think...","signature":"sig"},
                    {"type":"text","text":"Here's the answer."}
                ]
            }
        }"#;
        let event: Event = parse_one(json);
        let Event::Assistant(conv) = event else {
            panic!("expected Assistant variant");
        };
        assert_eq!(conv.message.model.as_deref(), Some("claude-opus-4-7"));
        assert_eq!(conv.message.stop_reason.as_deref(), Some("end_turn"));
        // content is an array; parse the blocks separately
        let blocks: Vec<ContentBlock> = serde_json::from_value(conv.message.content).unwrap();
        assert_eq!(blocks.len(), 2);
        assert!(matches!(blocks[0], ContentBlock::Thinking { .. }));
        assert!(matches!(blocks[1], ContentBlock::Text { .. }));
    }

    #[test]
    fn synthetic_assistant_api_error_parses() {
        // Schema-drift case: assistant with model "<synthetic>" + apiErrorStatus.
        let json = r#"{
            "type":"assistant",
            "sessionId":"s1",
            "isSidechain":false,
            "message":{
                "role":"assistant",
                "model":"<synthetic>",
                "content":[{"type":"text","text":"API Error: 529 overloaded"}]
            },
            "isApiErrorMessage":true,
            "apiErrorStatus":529,
            "error":{"code":"overloaded"}
        }"#;
        let event: Event = parse_one(json);
        let Event::Assistant(conv) = event else {
            panic!("expected Assistant variant");
        };
        assert!(conv.is_api_error_message);
        assert_eq!(conv.api_error_status, Some(529));
        assert_eq!(conv.message.model.as_deref(), Some("<synthetic>"));
    }

    #[test]
    fn system_stop_hook_summary_parses() {
        let json = r#"{
            "type":"system",
            "sessionId":"s1",
            "subtype":"stop_hook_summary",
            "isSidechain":false,
            "level":"info",
            "messageCount":5,
            "hookCount":2
        }"#;
        let event: Event = parse_one(json);
        let Event::System(sys) = event else {
            panic!("expected System variant");
        };
        assert_eq!(sys.subtype, "stop_hook_summary");
        assert_eq!(sys.message_count, Some(5));
        assert_eq!(sys.hook_count, Some(2));
    }

    #[test]
    fn system_compact_boundary_parses() {
        let json = r#"{
            "type":"system",
            "sessionId":"s1",
            "subtype":"compact_boundary",
            "logicalParentUuid":"u100",
            "isSidechain":false,
            "compactMetadata":{
                "trigger":"manual",
                "preTokens":95000,
                "postTokens":12000,
                "durationMs":5400
            },
            "slug":"my-session"
        }"#;
        let event: Event = parse_one(json);
        let Event::System(sys) = event else {
            panic!("expected System variant");
        };
        assert_eq!(sys.subtype, "compact_boundary");
        assert_eq!(sys.envelope.logical_parent_uuid.as_deref(), Some("u100"));
        assert!(sys.compact_metadata.is_some());
        assert_eq!(sys.slug.as_deref(), Some("my-session"));
    }

    #[test]
    fn system_api_error_retry_parses() {
        let json = r#"{
            "type":"system",
            "sessionId":"s1",
            "subtype":"api_error",
            "isSidechain":false,
            "cause":{"status":529,"message":"overloaded"},
            "retryAttempt":1,
            "retryInMs":2500.0,
            "maxRetries":5
        }"#;
        let event: Event = parse_one(json);
        let Event::System(sys) = event else {
            panic!("expected System variant");
        };
        assert_eq!(sys.subtype, "api_error");
        assert_eq!(sys.retry_attempt, Some(1));
        assert_eq!(sys.retry_in_ms, Some(2500.0));
        assert_eq!(sys.max_retries, Some(5));
    }

    #[test]
    fn last_prompt_event_parses() {
        let json = r#"{
            "type":"last-prompt",
            "lastPrompt":"What's the meaning of life?",
            "sessionId":"s1"
        }"#;
        let event: Event = parse_one(json);
        let Event::LastPrompt(lp) = event else {
            panic!("expected LastPrompt variant");
        };
        assert_eq!(lp.last_prompt, "What's the meaning of life?");
        assert_eq!(lp.session_id, "s1");
    }

    #[test]
    fn permission_mode_event_parses() {
        let json = r#"{"type":"permission-mode","permissionMode":"default","sessionId":"s1"}"#;
        let event: Event = parse_one(json);
        let Event::PermissionMode(pm) = event else {
            panic!("expected PermissionMode variant");
        };
        assert_eq!(pm.permission_mode, "default");
    }

    #[test]
    fn pr_link_event_parses() {
        let json = r#"{
            "type":"pr-link",
            "sessionId":"s1",
            "prNumber":42,
            "prUrl":"https://github.com/x/y/pull/42",
            "prRepository":"x/y",
            "timestamp":"2026-04-23T16:35:00.000Z"
        }"#;
        let event: Event = parse_one(json);
        let Event::PrLink(pr) = event else {
            panic!("expected PrLink variant");
        };
        assert_eq!(pr.pr_number, 42);
        assert_eq!(pr.pr_repository, "x/y");
    }

    #[test]
    fn queue_operation_event_parses() {
        let json = r#"{
            "type":"queue-operation",
            "operation":"enqueue",
            "content":"the next prompt",
            "sessionId":"s1",
            "timestamp":"2026-04-23T16:36:00.000Z"
        }"#;
        let event: Event = parse_one(json);
        let Event::QueueOperation(qo) = event else {
            panic!("expected QueueOperation variant");
        };
        assert_eq!(qo.operation, "enqueue");
        assert_eq!(qo.content.as_deref(), Some("the next prompt"));
    }

    #[test]
    fn attachment_event_parses() {
        let json = r#"{
            "type":"attachment",
            "sessionId":"s1",
            "isSidechain":false,
            "attachment":{"id":"a1","kind":"image","size":1024}
        }"#;
        let event: Event = parse_one(json);
        let Event::Attachment(att) = event else {
            panic!("expected Attachment variant");
        };
        assert_eq!(att.envelope.session_id, "s1");
        assert_eq!(att.attachment["id"], Value::String("a1".into()));
    }

    #[test]
    fn file_history_snapshot_event_parses() {
        let json = r#"{
            "type":"file-history-snapshot",
            "messageId":"m1",
            "snapshot":{"files":[]},
            "isSnapshotUpdate":false
        }"#;
        let event: Event = parse_one(json);
        let Event::FileHistorySnapshot(fhs) = event else {
            panic!("expected FileHistorySnapshot variant");
        };
        assert_eq!(fhs.message_id, "m1");
        assert!(!fhs.is_snapshot_update);
    }

    #[test]
    fn sidechain_flag_parses() {
        let json = r#"{
            "type":"user",
            "sessionId":"s1",
            "isSidechain":true,
            "parentUuid":"main-uuid",
            "message":{"role":"user","content":"sub-agent prompt"}
        }"#;
        let event: Event = parse_one(json);
        let Event::User(conv) = event else {
            panic!("expected User variant");
        };
        assert!(conv.envelope.is_sidechain);
    }

    #[test]
    fn missing_optional_envelope_fields_default_correctly() {
        // Bookkeeping events skip cwd/version/userType. Should still parse.
        let json = r#"{
            "type":"user",
            "sessionId":"s1",
            "message":{"role":"user","content":"x"}
        }"#;
        let event: Event = parse_one(json);
        let Event::User(conv) = event else {
            panic!("expected User variant");
        };
        assert_eq!(conv.envelope.cwd, None);
        assert_eq!(conv.envelope.version, None);
        assert_eq!(conv.envelope.user_type, None);
        assert!(!conv.envelope.is_sidechain); // default
        assert!(!conv.is_api_error_message); // default
    }

    #[test]
    fn unknown_top_level_type_is_a_parse_error() {
        let json = r#"{"type":"future-event-we-have-not-seen","sessionId":"s1"}"#;
        let result: Result<Event, _> = serde_json::from_str(json);
        assert!(
            result.is_err(),
            "unknown variant should fail to parse so read() can warn-and-skip it"
        );
    }

    #[test]
    fn read_handles_mixed_real_world_lines() {
        let lines = [
            // user
            r#"{"type":"user","sessionId":"s","message":{"role":"user","content":"hi"}}"#,
            // assistant
            r#"{"type":"assistant","sessionId":"s","message":{"role":"assistant","model":"claude-opus-4-7","content":[{"type":"text","text":"hello"}]}}"#,
            // empty line
            "",
            // system
            r#"{"type":"system","sessionId":"s","subtype":"turn_duration","durationMs":1234}"#,
            // bookkeeping
            r#"{"type":"last-prompt","lastPrompt":"hi","sessionId":"s"}"#,
        ];
        let tmp = write_jsonl(&lines);
        let events = read(tmp.path()).unwrap();
        assert_eq!(events.len(), 4); // empty line skipped
    }

    #[test]
    fn read_skips_malformed_lines_without_failing() {
        let lines = [
            r#"{"type":"user","sessionId":"s","message":{"role":"user","content":"good"}}"#,
            "this is not even json",
            r#"{"type":"future-unknown-variant","sessionId":"s"}"#, // also skipped
            r#"{"type":"user","sessionId":"s","message":{"role":"user","content":"also good"}}"#,
        ];
        let tmp = write_jsonl(&lines);
        let events = read(tmp.path()).unwrap();
        assert_eq!(
            events.len(),
            2,
            "should keep only the two well-formed user events"
        );
    }

    #[test]
    fn read_returns_io_error_for_missing_file() {
        let path = std::path::Path::new("/nonexistent/path/to/transcript.jsonl");
        let err = read(path).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("failed to open transcript"),
            "expected open-context wrapper; got: {msg}"
        );
    }

    #[test]
    fn read_handles_empty_file() {
        let tmp = tempfile::Builder::new()
            .suffix(".jsonl")
            .tempfile()
            .unwrap();
        let events = read(tmp.path()).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn content_block_polymorphism_parses_each_variant() {
        let blocks = r#"[
            {"type":"text","text":"hi"},
            {"type":"thinking","thinking":"hmm"},
            {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}},
            {"type":"tool_result","tool_use_id":"t1","content":"output","is_error":false}
        ]"#;
        let parsed: Vec<ContentBlock> = serde_json::from_str(blocks).unwrap();
        assert_eq!(parsed.len(), 4);
        assert!(matches!(parsed[0], ContentBlock::Text { .. }));
        assert!(matches!(parsed[1], ContentBlock::Thinking { .. }));
        assert!(matches!(parsed[2], ContentBlock::ToolUse { .. }));
        assert!(matches!(parsed[3], ContentBlock::ToolResult { .. }));
    }

    #[test]
    fn round_trip_user_event() {
        let json = r#"{"type":"user","sessionId":"s","isSidechain":false,"message":{"role":"user","content":"hi"}}"#;
        let event: Event = parse_one(json);
        let serialized = serde_json::to_string(&event).unwrap();
        let again: Event = parse_one(&serialized);
        let Event::User(conv) = again else {
            panic!("round-trip variant lost");
        };
        assert_eq!(conv.envelope.session_id, "s");
    }
}
