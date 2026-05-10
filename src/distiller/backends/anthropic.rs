//! Anthropic Messages API backend (SSE streaming).
//!
//! Direct HTTP to `api.anthropic.com/v1/messages` with `stream: true`.
//! Used when the user has `$ANTHROPIC_API_KEY` set (or has stored a key
//! via `alluvium init`).
//!
//! ## Why streaming
//!
//! Distillation responses are 1-4 KB of JSON, returned in a few seconds
//! to ~30 seconds. Non-streaming works, but streaming buys two things:
//!
//! 1. The connection can't time out at an intermediate proxy that
//!    expects bytes within N seconds of the request — chunks flow
//!    continuously while the model thinks.
//! 2. Token usage in `message_delta` lands before the final `[DONE]`,
//!    so we get accurate `output_tokens` even on slow trailing chunks.
//!
//! ## SSE event types we care about
//!
//! - `message_start` — initial `input_tokens`
//! - `content_block_delta` — incremental `delta.text` we accumulate
//! - `message_delta` — final `output_tokens`
//! - `message_stop` — end-of-stream marker
//! - `error` — surfaced as Result::Err
//!
//! Every other event type (`ping`, `content_block_start`,
//! `content_block_stop`) is silently ignored — they carry no data we need.

use anyhow::{Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;

use crate::distiller::backend::{LlmBackend, LlmResponse, RenderedPrompt};
use crate::distiller::TokenUsage;

const API_VERSION: &str = "2023-06-01";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const DEFAULT_MODEL: &str = "claude-haiku-4-5";

#[derive(Debug, Clone)]
pub struct AnthropicBackend {
    api_key: String,
    base_url: String,
    default_model: String,
    http: reqwest::Client,
}

impl AnthropicBackend {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: DEFAULT_BASE_URL.into(),
            default_model: DEFAULT_MODEL.into(),
            http: reqwest::Client::new(),
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }
}

#[async_trait]
impl LlmBackend for AnthropicBackend {
    fn kind(&self) -> &'static str {
        "anthropic"
    }

    async fn complete(&self, prompt: &RenderedPrompt) -> Result<LlmResponse> {
        let model = prompt.model.as_deref().unwrap_or(&self.default_model);
        let url = format!("{}/v1/messages", self.base_url);
        let body = serde_json::json!({
            "model": model,
            "max_tokens": prompt.max_tokens,
            "stream": true,
            "system": prompt.system,
            "messages": [
                {"role": "user", "content": prompt.user}
            ]
        });

        let response = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .header("accept", "text/event-stream")
            .json(&body)
            .send()
            .await
            .with_context(|| format!("HTTP request to {url} failed"))?;

        let status = response.status();
        if !status.is_success() {
            let body_text = response
                .text()
                .await
                .context("reading Anthropic error response body")?;
            let preview: String = body_text.chars().take(500).collect();
            anyhow::bail!("Anthropic API returned {status}: {preview}");
        }

        let (text, usage) = consume_sse(response).await?;
        if text.is_empty() {
            anyhow::bail!("Anthropic stream had no assistant text content");
        }

        Ok(LlmResponse { text, usage })
    }
}

/// Drive a reqwest streaming response as an Anthropic SSE event stream.
/// Returns the accumulated assistant text + token usage.
///
/// Pulled out of `complete` so the SSE parsing has direct unit-test
/// coverage via [`parse_sse_payload`] without going through the HTTP
/// layer (mockito covers the integration side).
async fn consume_sse(response: reqwest::Response) -> Result<(String, Option<TokenUsage>)> {
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::<u8>::new();
    let mut state = SseState::default();

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.context("reading Anthropic SSE chunk")?;
        buffer.extend_from_slice(&chunk);

        // SSE events are separated by a blank line (\n\n). Drain whole
        // events out of the buffer and feed them to the parser; leave
        // any partial trailing event for the next chunk.
        while let Some(split) = find_event_boundary(&buffer) {
            let event_bytes = buffer.drain(..split).collect::<Vec<u8>>();
            // Drop the boundary itself (\n\n or \r\n\r\n).
            if buffer.starts_with(b"\r\n\r\n") {
                buffer.drain(..4);
            } else if buffer.starts_with(b"\n\n") {
                buffer.drain(..2);
            }
            let event_str = match std::str::from_utf8(&event_bytes) {
                Ok(s) => s,
                Err(e) => anyhow::bail!("Anthropic SSE event was not valid UTF-8: {e}"),
            };
            parse_sse_payload(event_str, &mut state)?;
            if state.stopped {
                return Ok(state.into_result());
            }
        }
    }

    Ok(state.into_result())
}

/// Find the index of the first `\n\n` (or `\r\n\r\n`) sequence in `buf`.
/// `None` means the buffer holds at most a partial event so far.
fn find_event_boundary(buf: &[u8]) -> Option<usize> {
    // Manual search — avoid pulling regex for this; the payloads are
    // small enough that a linear scan is fine.
    let mut i = 0;
    while i + 1 < buf.len() {
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some(i);
        }
        if i + 3 < buf.len()
            && buf[i] == b'\r'
            && buf[i + 1] == b'\n'
            && buf[i + 2] == b'\r'
            && buf[i + 3] == b'\n'
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[derive(Debug, Default)]
struct SseState {
    text: String,
    input_tokens: u64,
    output_tokens: u64,
    saw_usage: bool,
    stopped: bool,
}

impl SseState {
    fn into_result(self) -> (String, Option<TokenUsage>) {
        let usage = if self.saw_usage {
            Some(TokenUsage {
                input_tokens: self.input_tokens,
                output_tokens: self.output_tokens,
            })
        } else {
            None
        };
        (self.text, usage)
    }
}

/// Parse one SSE event block (multi-line, lines like `event: foo` and
/// `data: {...}`) and update `state` accordingly.
fn parse_sse_payload(event_block: &str, state: &mut SseState) -> Result<()> {
    let mut event_name = "";
    let mut data_buf = String::new();

    for line in event_block.lines() {
        if let Some(rest) = line.strip_prefix("event: ") {
            event_name = rest;
        } else if let Some(rest) = line.strip_prefix("event:") {
            event_name = rest.trim_start();
        } else if let Some(rest) = line.strip_prefix("data: ") {
            if !data_buf.is_empty() {
                data_buf.push('\n');
            }
            data_buf.push_str(rest);
        } else if let Some(rest) = line.strip_prefix("data:") {
            if !data_buf.is_empty() {
                data_buf.push('\n');
            }
            data_buf.push_str(rest.trim_start());
        }
        // `:` comments and other field types are ignored per SSE spec.
    }

    if data_buf.is_empty() {
        return Ok(());
    }

    let json: serde_json::Value = match serde_json::from_str(&data_buf) {
        Ok(v) => v,
        Err(_) => return Ok(()), // tolerate the occasional non-JSON keepalive
    };

    match event_name {
        "message_start" => {
            if let Some(u) = json.pointer("/message/usage") {
                if let Some(it) = u.get("input_tokens").and_then(|v| v.as_u64()) {
                    state.input_tokens = it;
                    state.saw_usage = true;
                }
                if let Some(ot) = u.get("output_tokens").and_then(|v| v.as_u64()) {
                    state.output_tokens = ot;
                    state.saw_usage = true;
                }
            }
        }
        "content_block_delta" => {
            if let Some(t) = json.pointer("/delta/text").and_then(|v| v.as_str()) {
                state.text.push_str(t);
            }
        }
        "message_delta" => {
            if let Some(ot) = json
                .pointer("/usage/output_tokens")
                .and_then(|v| v.as_u64())
            {
                state.output_tokens = ot;
                state.saw_usage = true;
            }
        }
        "message_stop" => {
            state.stopped = true;
        }
        "error" => {
            let preview = json.to_string();
            let preview: String = preview.chars().take(500).collect();
            anyhow::bail!("Anthropic stream returned error event: {preview}");
        }
        _ => {} // ping / content_block_start / content_block_stop — ignore
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    fn sample_prompt() -> RenderedPrompt {
        RenderedPrompt {
            system: "You are a librarian.".into(),
            user: "Distill this transcript.".into(),
            model: None,
            max_tokens: 1024,
        }
    }

    /// SSE wire bytes simulating a small streamed response.
    fn happy_sse_body() -> String {
        // Three deltas + final usage, exactly as Anthropic would send it.
        [
            r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","model":"claude-haiku-4-5","content":[],"usage":{"input_tokens":42,"output_tokens":1}}}

"#,
            r#"event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello "}}

"#,
            r#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"world"}}

"#,
            r#"event: content_block_stop
data: {"type":"content_block_stop","index":0}

"#,
            r#"event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":7}}

"#,
            r#"event: message_stop
data: {"type":"message_stop"}

"#,
        ]
        .concat()
    }

    #[tokio::test]
    async fn happy_path_streams_text_and_usage() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(happy_sse_body())
            .create_async()
            .await;
        let backend = AnthropicBackend::new("k".into()).with_base_url(server.url());
        let r = backend.complete(&sample_prompt()).await.unwrap();
        assert_eq!(r.text, "hello world");
        let u = r.usage.unwrap();
        assert_eq!(u.input_tokens, 42);
        assert_eq!(u.output_tokens, 7);
    }

    #[tokio::test]
    async fn http_4xx_returns_status_in_error() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(401)
            .with_body(r#"{"error":{"message":"bad key"}}"#)
            .create_async()
            .await;
        let backend = AnthropicBackend::new("k".into()).with_base_url(server.url());
        let err = backend.complete(&sample_prompt()).await.unwrap_err();
        assert!(format!("{err:#}").contains("401"));
    }

    #[tokio::test]
    async fn empty_stream_errors() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            // Stream that contains a message_start + message_stop but no
            // text deltas — final accumulated text is empty.
            .with_body(
                "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
            )
            .create_async()
            .await;
        let backend = AnthropicBackend::new("k".into()).with_base_url(server.url());
        let err = backend.complete(&sample_prompt()).await.unwrap_err();
        assert!(format!("{err:#}").contains("no assistant text"));
    }

    #[tokio::test]
    async fn model_override_propagates_in_request() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "model": "claude-sonnet-4-6",
                "stream": true
            })))
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            .with_body(happy_sse_body())
            .create_async()
            .await;
        let backend = AnthropicBackend::new("k".into()).with_base_url(server.url());
        let mut p = sample_prompt();
        p.model = Some("claude-sonnet-4-6".into());
        backend.complete(&p).await.unwrap();
        mock.assert_async().await;
    }

    #[test]
    fn parse_sse_accumulates_text_deltas() {
        let mut state = SseState::default();
        parse_sse_payload(
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}",
            &mut state,
        )
        .unwrap();
        parse_sse_payload(
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" there\"}}",
            &mut state,
        )
        .unwrap();
        assert_eq!(state.text, "hi there");
    }

    #[test]
    fn parse_sse_error_event_propagates() {
        let mut state = SseState::default();
        let err = parse_sse_payload(
            "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded\",\"message\":\"slow down\"}}",
            &mut state,
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("error event"));
    }

    #[test]
    fn parse_sse_ignores_ping() {
        let mut state = SseState::default();
        parse_sse_payload("event: ping\ndata: {}", &mut state).unwrap();
        assert!(state.text.is_empty());
        assert!(!state.saw_usage);
    }

    #[test]
    fn find_event_boundary_finds_lf_lf_and_crlf_pair() {
        assert_eq!(find_event_boundary(b"a\n\nb"), Some(1));
        assert_eq!(find_event_boundary(b"a\r\n\r\nb"), Some(1));
        assert_eq!(find_event_boundary(b"abc"), None);
    }
}
