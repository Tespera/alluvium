//! Convert a parsed transcript into a clean conversation flow.
//!
//! Walks the [`Event`] stream and emits a `Vec<Message>` shaped for the
//! distiller. The reduction does three things:
//!
//! 1. **Filters noise**: sidechain (sub-agent) events, system events,
//!    attachments, and bookkeeping (last-prompt / permission-mode / pr-link /
//!    queue-operation / file-history-snapshot) are all dropped.
//! 2. **Pairs tool_use with tool_result**: tool_result blocks live inside
//!    *user* events (Claude Code's transport convention). We index them
//!    by `tool_use_id` and attach the result to the originating
//!    assistant `tool_use` block as `ToolCall.output`. The pure
//!    tool_result user-event itself is dropped — its content has already
//!    flowed into the assistant's [`ToolCall`].
//! 3. **Flattens polymorphic content**: `message.content` may be a bare
//!    string (typed user prompt), an array of `text` blocks, or a mix of
//!    `text` / `thinking` / `tool_use` / `tool_result`. We concatenate the
//!    `text` blocks (joined by `\n\n`), drop `thinking`, lift `tool_use`
//!    into `tool_calls`.
//!
//! Output: `Vec<Message>` from [`crate::transcript`]. Empty messages
//! (no text and no tool_calls) are dropped to keep the distiller's
//! input lean.

use std::collections::HashMap;

use serde_json::Value;

use crate::transcript::jsonl::{ContentBlock, Event};
use crate::transcript::{Message, ToolCall};

/// Reduce a parsed transcript into a clean conversation flow.
pub fn run(events: &[Event]) -> Vec<Message> {
    let tool_results = collect_tool_results(events);

    let mut messages = Vec::new();
    for event in events {
        let (role, conv) = match event {
            Event::User(c) if !c.envelope.is_sidechain => ("user", c),
            Event::Assistant(c) if !c.envelope.is_sidechain => ("assistant", c),
            _ => continue,
        };

        let blocks = parse_content_blocks(&conv.message.content);

        let text = match &conv.message.content {
            // Typed user prompts arrive as bare strings; preserve verbatim.
            Value::String(s) => s.clone(),
            // Otherwise we already parsed the array into typed blocks.
            _ => blocks_to_text(&blocks),
        };

        let tool_calls = if role == "assistant" {
            blocks_to_tool_calls(&blocks, &tool_results)
        } else {
            // User messages may carry tool_result blocks but never tool_use;
            // the results have been collected separately.
            Vec::new()
        };

        if text.is_empty() && tool_calls.is_empty() {
            // Drop pure-tool_result user events and any other empty messages.
            continue;
        }

        messages.push(Message {
            role: role.to_string(),
            content: text,
            tool_calls,
        });
    }

    messages
}

/// Build a `tool_use_id -> stringified-result` map by scanning all main-thread
/// user events for `tool_result` blocks.
fn collect_tool_results(events: &[Event]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for event in events {
        let Event::User(c) = event else { continue };
        if c.envelope.is_sidechain {
            continue;
        }
        for block in parse_content_blocks(&c.message.content) {
            if let ContentBlock::ToolResult {
                tool_use_id,
                content,
                ..
            } = block
            {
                map.insert(tool_use_id, stringify_tool_result(&content));
            }
        }
    }
    map
}

/// Parse the array form of `message.content` into typed [`ContentBlock`]s.
/// String / null / object content yields an empty Vec.
fn parse_content_blocks(content: &Value) -> Vec<ContentBlock> {
    match content {
        Value::Array(_) => serde_json::from_value(content.clone()).unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Concatenate `Text` block bodies, joined by a blank line. Other block
/// types (`thinking`, `tool_use`, `tool_result`) are ignored — `thinking`
/// is internal to the model, and the tool blocks are surfaced via
/// [`ToolCall`].
fn blocks_to_text(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Lift each `ToolUse` block into a [`ToolCall`], attaching its `output`
/// from the precomputed results map (or `None` when the transcript
/// truncated before the result arrived).
fn blocks_to_tool_calls(
    blocks: &[ContentBlock],
    results: &HashMap<String, String>,
) -> Vec<ToolCall> {
    blocks
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolUse {
                id, name, input, ..
            } => {
                let output = results.get(id).cloned();
                Some(ToolCall {
                    name: name.clone(),
                    input: input.clone(),
                    output,
                })
            }
            _ => None,
        })
        .collect()
}

/// `tool_result.content` is itself polymorphic: it can be a bare string, or
/// an array of objects (often `{"type":"text","text":"..."}` blocks, but
/// occasionally images or arbitrary tool-specific shapes). We flatten it
/// to a single string here so downstream sees a uniform shape.
fn stringify_tool_result(content: &Value) -> String {
    match content {
        Value::String(s) => s.clone(),
        Value::Array(arr) => arr
            .iter()
            .filter_map(|item| {
                if item.get("type") == Some(&Value::String("text".into())) {
                    item.get("text").and_then(|v| v.as_str()).map(String::from)
                } else {
                    // Non-text item: serialize the JSON as a fallback so
                    // we don't silently lose information.
                    Some(item.to_string())
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => content.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn evt(json: &str) -> Event {
        serde_json::from_str(json).expect("test event JSON should parse")
    }

    // ─────────────────── basic shape ───────────────────

    #[test]
    fn user_string_content_becomes_message() {
        let events = vec![evt(
            r#"{"type":"user","sessionId":"S","message":{"role":"user","content":"hello"}}"#,
        )];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "hello");
        assert!(msgs[0].tool_calls.is_empty());
    }

    #[test]
    fn assistant_text_block_becomes_content() {
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","model":"claude-opus-4-7","content":[{"type":"text","text":"hi there"}]}}"#,
        )];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "assistant");
        assert_eq!(msgs[0].content, "hi there");
    }

    #[test]
    fn multiple_text_blocks_joined_by_blank_line() {
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"text","text":"para 1"},{"type":"text","text":"para 2"}]}}"#,
        )];
        let msgs = run(&events);
        assert_eq!(msgs[0].content, "para 1\n\npara 2");
    }

    #[test]
    fn empty_input_returns_empty() {
        let msgs = run(&[]);
        assert!(msgs.is_empty());
    }

    #[test]
    fn empty_assistant_message_is_dropped() {
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[]}}"#,
        )];
        let msgs = run(&events);
        assert!(msgs.is_empty());
    }

    // ─────────────────── thinking blocks ───────────────────

    #[test]
    fn assistant_thinking_blocks_are_dropped() {
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[
                {"type":"thinking","thinking":"private reasoning"},
                {"type":"text","text":"public answer"}
            ]}}"#,
        )];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "public answer");
        assert!(!msgs[0].content.contains("private"));
    }

    // ─────────────────── tool_use / tool_result pairing ───────────────────

    #[test]
    fn tool_use_without_matching_result_has_none_output() {
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[
                {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}
            ]}}"#,
        )];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].tool_calls.len(), 1);
        assert_eq!(msgs[0].tool_calls[0].name, "Bash");
        assert_eq!(msgs[0].tool_calls[0].input["command"], "ls");
        assert!(msgs[0].tool_calls[0].output.is_none());
    }

    #[test]
    fn tool_result_pairs_with_tool_use_via_id() {
        let events = vec![
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}]}}"#,
            ),
            evt(
                r#"{"type":"user","sessionId":"S","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"file1.txt\nfile2.txt"}]}}"#,
            ),
        ];
        let msgs = run(&events);
        // Pure-tool_result user message is dropped; its content flows into
        // the assistant's ToolCall.output.
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "assistant");
        assert_eq!(
            msgs[0].tool_calls[0].output.as_deref(),
            Some("file1.txt\nfile2.txt")
        );
    }

    #[test]
    fn tool_result_with_array_text_blocks_flattens() {
        let events = vec![
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#,
            ),
            evt(
                r#"{"type":"user","sessionId":"S","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"line 1"},{"type":"text","text":"line 2"}]}]}}"#,
            ),
        ];
        let msgs = run(&events);
        assert_eq!(
            msgs[0].tool_calls[0].output.as_deref(),
            Some("line 1\nline 2")
        );
    }

    #[test]
    fn tool_result_with_unknown_block_type_serializes_to_json_fallback() {
        let events = vec![
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"X","input":{}}]}}"#,
            ),
            // Tool result is an array of an "image" block (no "text" field).
            evt(
                r#"{"type":"user","sessionId":"S","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"image","source":{"data":"abc"}}]}]}}"#,
            ),
        ];
        let msgs = run(&events);
        let out = msgs[0].tool_calls[0].output.as_deref().unwrap();
        assert!(
            out.contains("image") && out.contains("abc"),
            "expected JSON fallback to preserve fields; got: {out}"
        );
    }

    #[test]
    fn assistant_text_and_tool_use_combined_in_same_message() {
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[
                {"type":"text","text":"Let me check that for you."},
                {"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}
            ]}}"#,
        )];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "Let me check that for you.");
        assert_eq!(msgs[0].tool_calls.len(), 1);
    }

    #[test]
    fn pure_tool_result_user_message_is_dropped() {
        let events = vec![
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
            ),
            evt(
                r#"{"type":"user","sessionId":"S","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"output"}]}}"#,
            ),
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"text","text":"done"}]}}"#,
            ),
        ];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs.iter().filter(|m| m.role == "user").count(), 0);
    }

    #[test]
    fn mixed_user_message_with_text_and_tool_result_keeps_text() {
        let events = vec![
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
            ),
            evt(
                r#"{"type":"user","sessionId":"S","message":{"role":"user","content":[
                {"type":"tool_result","tool_use_id":"t1","content":"output"},
                {"type":"text","text":"please run another"}
            ]}}"#,
            ),
        ];
        let msgs = run(&events);
        let user_msgs: Vec<_> = msgs.iter().filter(|m| m.role == "user").collect();
        assert_eq!(user_msgs.len(), 1);
        assert_eq!(user_msgs[0].content, "please run another");
    }

    // ─────────────────── filtering ───────────────────

    #[test]
    fn sidechain_events_are_filtered() {
        let events = vec![
            evt(
                r#"{"type":"user","sessionId":"S","message":{"role":"user","content":"main thread"}}"#,
            ),
            evt(
                r#"{"type":"user","sessionId":"S","isSidechain":true,"message":{"role":"user","content":"subagent thread"}}"#,
            ),
            evt(
                r#"{"type":"assistant","sessionId":"S","isSidechain":true,"message":{"role":"assistant","content":[{"type":"text","text":"subagent reply"}]}}"#,
            ),
        ];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "main thread");
    }

    #[test]
    fn sidechain_tool_result_does_not_leak_into_main_thread_calls() {
        // A main-thread tool_use should NOT pick up a sidechain user's
        // tool_result, even if the ids happen to collide.
        let events = vec![
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
            ),
            evt(
                r#"{"type":"user","sessionId":"S","isSidechain":true,"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"sidechain output"}]}}"#,
            ),
        ];
        let msgs = run(&events);
        assert!(msgs[0].tool_calls[0].output.is_none());
    }

    #[test]
    fn system_attachment_bookkeeping_events_are_filtered() {
        let events = vec![
            evt(r#"{"type":"system","sessionId":"S","subtype":"turn_duration","durationMs":1234}"#),
            evt(r#"{"type":"attachment","sessionId":"S","attachment":{"id":"a1"}}"#),
            evt(r#"{"type":"last-prompt","lastPrompt":"hi","sessionId":"S"}"#),
            evt(
                r#"{"type":"file-history-snapshot","messageId":"m1","snapshot":{},"isSnapshotUpdate":false}"#,
            ),
            evt(r#"{"type":"permission-mode","permissionMode":"default","sessionId":"S"}"#),
            evt(
                r#"{"type":"pr-link","sessionId":"S","prNumber":1,"prUrl":"x","prRepository":"y"}"#,
            ),
            evt(r#"{"type":"queue-operation","operation":"enqueue","sessionId":"S"}"#),
            evt(
                r#"{"type":"user","sessionId":"S","message":{"role":"user","content":"the only real message"}}"#,
            ),
        ];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "the only real message");
    }

    // ─────────────────── special cases ───────────────────

    #[test]
    fn synthetic_api_error_assistant_still_passes_through() {
        // Schema-drift case: the model field is "<synthetic>" and there's
        // an isApiErrorMessage flag. The text content is still relevant
        // ("API Error: ..."), so we keep the message.
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","model":"<synthetic>","content":[{"type":"text","text":"API Error: 529 overloaded"}]},"isApiErrorMessage":true,"apiErrorStatus":529}"#,
        )];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].content.contains("529"));
    }

    #[test]
    fn order_of_messages_is_preserved() {
        let events = vec![
            evt(r#"{"type":"user","sessionId":"S","message":{"role":"user","content":"first"}}"#),
            evt(
                r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"text","text":"second"}]}}"#,
            ),
            evt(r#"{"type":"user","sessionId":"S","message":{"role":"user","content":"third"}}"#),
        ];
        let msgs = run(&events);
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0].content, "first");
        assert_eq!(msgs[1].content, "second");
        assert_eq!(msgs[2].content, "third");
    }

    #[test]
    fn malformed_content_array_does_not_panic() {
        // The Event already deserialized successfully (jsonl::read parsed
        // it), so message.content is some Value::Array. But its element
        // shapes might surprise us — make sure we degrade gracefully.
        let events = vec![evt(
            r#"{"type":"assistant","sessionId":"S","message":{"role":"assistant","content":[{"type":"text","text":"valid"},{"type":"unknown_block","weird_field":42}]}}"#,
        )];
        // The "unknown_block" entry will fail ContentBlock deserialization.
        // We currently fall back to an empty Vec on parse failure, which
        // means we'd lose the "valid" text too. That's a known limitation
        // of v0.1; downstream still gets the assistant message via
        // tool_calls if any, or it's dropped silently.
        let msgs = run(&events);
        // The whole content array fails to parse → message becomes empty
        // → it's dropped. Better than panicking.
        assert!(msgs.is_empty() || msgs[0].content == "valid");
    }
}
