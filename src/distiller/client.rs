//! Anthropic Messages API client.
//!
//! Plain `reqwest` over HTTPS to `api.anthropic.com/v1/messages`. No
//! third-party SDK — keeps the dependency surface minimal and avoids
//! tracking immature Rust crates.
//!
//! ## What's in scope (v0.1)
//!
//! - Single `messages()` method. Sends a JSON body as-is, returns the
//!   parsed JSON response as-is. Distiller-level types live in
//!   `parser.rs`; this module is purely transport.
//! - All three required headers (`x-api-key`, `anthropic-version`,
//!   `content-type`).
//! - HTTP errors surface with status + body preview (truncated to 500
//!   chars) so a 529 overloaded comes back as a human-readable error.
//! - Configurable `base_url` so unit tests can point at a local mock
//!   server (mockito).
//!
//! ## What's deferred to v0.2+
//!
//! - Retry / backoff on 429 / 529 / 503. Caller handles for now.
//! - SSE streaming (`stream: true`). Distillation is fine non-streamed.
//! - Prompt caching headers. Defer until we have a concrete distiller
//!   prompt to cache.

use anyhow::{Context, Result};

const API_VERSION: &str = "2023-06-01";
const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

/// Anthropic API client.
#[derive(Debug, Clone)]
pub struct AnthropicClient {
    api_key: String,
    base_url: String,
    http: reqwest::Client,
}

impl AnthropicClient {
    /// Construct a client targeting the real Anthropic API with the given key.
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: DEFAULT_BASE_URL.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// For tests: redirect requests to a mock server. Returns `self` for
    /// chaining.
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    /// POST to `/v1/messages` and return the response body parsed as JSON.
    ///
    /// The `request` is sent as-is — the caller is responsible for shaping
    /// the body (`model`, `max_tokens`, `messages`, `system`, …). On non-2xx
    /// the call returns an error that names the HTTP status and includes a
    /// preview of the response body so transient failures (rate limits,
    /// overloaded backends) are diagnosable from the audit log.
    pub async fn messages(&self, request: serde_json::Value) -> Result<serde_json::Value> {
        let url = format!("{}/v1/messages", self.base_url);
        let response = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .with_context(|| format!("HTTP request to Anthropic API ({url}) failed"))?;

        let status = response.status();
        let body = response
            .text()
            .await
            .context("failed to read Anthropic response body")?;

        if !status.is_success() {
            let preview: String = body.chars().take(500).collect();
            anyhow::bail!("Anthropic API returned {status}: {preview}");
        }

        serde_json::from_str(&body).with_context(|| {
            let preview: String = body.chars().take(500).collect();
            format!("failed to parse Anthropic response as JSON; first 500 chars: {preview}")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[tokio::test]
    async fn successful_response_is_parsed_to_json_value() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"text","text":"hello"}],"model":"claude-haiku-4-5","stop_reason":"end_turn","usage":{"input_tokens":5,"output_tokens":2}}"#,
            )
            .create_async()
            .await;

        let client = AnthropicClient::new("test-key".into()).with_base_url(server.url());
        let response = client
            .messages(serde_json::json!({
                "model": "claude-haiku-4-5",
                "max_tokens": 100,
                "messages": [{"role": "user", "content": "hi"}]
            }))
            .await
            .unwrap();

        assert_eq!(response["id"], "msg_1");
        assert_eq!(response["content"][0]["text"], "hello");
        assert_eq!(response["usage"]["input_tokens"], 5);
    }

    #[tokio::test]
    async fn required_headers_are_sent() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_header("x-api-key", "secret-key-value")
            .match_header("anthropic-version", "2023-06-01")
            .match_header("content-type", "application/json")
            .with_status(200)
            .with_body(r#"{"id":"x","type":"message","role":"assistant","content":[],"model":"x","usage":{"input_tokens":0,"output_tokens":0}}"#)
            .create_async()
            .await;

        let client = AnthropicClient::new("secret-key-value".into()).with_base_url(server.url());
        client.messages(serde_json::json!({})).await.unwrap();
        // mockito's match_header will fail the test if any required header was missing.
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn request_body_is_forwarded_as_json() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1/messages")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "model": "claude-opus-4-7",
                "messages": [{"role": "user", "content": "ping"}]
            })))
            .with_status(200)
            .with_body(r#"{"id":"x","type":"message","role":"assistant","content":[],"model":"x","usage":{"input_tokens":0,"output_tokens":0}}"#)
            .create_async()
            .await;

        let client = AnthropicClient::new("k".into()).with_base_url(server.url());
        client
            .messages(serde_json::json!({
                "model": "claude-opus-4-7",
                "max_tokens": 1000,
                "messages": [{"role": "user", "content": "ping"}]
            }))
            .await
            .unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn http_4xx_returns_error_with_status_and_body_preview() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(401)
            .with_body(r#"{"error":{"type":"authentication_error","message":"invalid x-api-key"}}"#)
            .create_async()
            .await;

        let client = AnthropicClient::new("bad-key".into()).with_base_url(server.url());
        let err = client.messages(serde_json::json!({})).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("401"), "status missing; got: {msg}");
        assert!(
            msg.contains("authentication_error"),
            "body preview missing; got: {msg}"
        );
    }

    #[tokio::test]
    async fn http_5xx_overloaded_is_diagnosable() {
        // 529 is the actual overloaded code we see in real Claude Code transcripts.
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(529)
            .with_body(r#"{"error":{"type":"overloaded","message":"server overloaded"}}"#)
            .create_async()
            .await;

        let client = AnthropicClient::new("k".into()).with_base_url(server.url());
        let err = client.messages(serde_json::json!({})).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("529") || msg.contains("overloaded"));
    }

    #[tokio::test]
    async fn malformed_json_response_errors_with_preview() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_body("this is not even json")
            .create_async()
            .await;

        let client = AnthropicClient::new("k".into()).with_base_url(server.url());
        let err = client.messages(serde_json::json!({})).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("failed to parse"),
            "expected our parse-context wrapper; got: {msg}"
        );
    }

    #[tokio::test]
    async fn error_body_preview_is_bounded_at_500_chars() {
        // Send back a huge error body. Our error message should not embed
        // all of it (otherwise audit logs blow up).
        let huge_body = "X".repeat(10_000);
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(500)
            .with_body(huge_body)
            .create_async()
            .await;

        let client = AnthropicClient::new("k".into()).with_base_url(server.url());
        let err = client.messages(serde_json::json!({})).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.len() < 1500,
            "error message should bound body preview; got {} chars",
            msg.len()
        );
    }

    #[tokio::test]
    async fn unreachable_base_url_returns_useful_error() {
        // Port 1 is reserved on most systems; connection should fail fast.
        let client = AnthropicClient::new("k".into()).with_base_url("http://127.0.0.1:1");
        let err = client.messages(serde_json::json!({})).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.contains("HTTP request to Anthropic API"),
            "expected our network-context wrapper; got: {msg}"
        );
    }
}
