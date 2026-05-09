//! Anthropic Messages API backend.
//!
//! Direct HTTP to `api.anthropic.com/v1/messages`. Used when the user has
//! `$ANTHROPIC_API_KEY` set (or has stored a key via `alluvium init`).

use anyhow::{Context, Result};
use async_trait::async_trait;

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
            .json(&body)
            .send()
            .await
            .with_context(|| format!("HTTP request to {url} failed"))?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .context("reading Anthropic response body")?;

        if !status.is_success() {
            let preview: String = body_text.chars().take(500).collect();
            anyhow::bail!("Anthropic API returned {status}: {preview}");
        }

        let parsed: serde_json::Value = serde_json::from_str(&body_text).with_context(|| {
            let preview: String = body_text.chars().take(500).collect();
            format!("Anthropic response was not valid JSON; first 500 chars: {preview}")
        })?;

        // Extract assistant text from content blocks (concatenate if multiple).
        let text = parsed
            .get("content")
            .and_then(|c| c.as_array())
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| {
                        if b.get("type")? == &serde_json::Value::String("text".into()) {
                            b.get("text")?.as_str()
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();

        if text.is_empty() {
            anyhow::bail!("Anthropic response had no assistant text content");
        }

        let usage = parsed.get("usage").and_then(|u| {
            let input = u.get("input_tokens")?.as_u64()?;
            let output = u.get("output_tokens")?.as_u64()?;
            Some(TokenUsage {
                input_tokens: input,
                output_tokens: output,
            })
        });

        Ok(LlmResponse { text, usage })
    }
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

    #[tokio::test]
    async fn happy_path_extracts_text_and_usage() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_body(r#"{"id":"x","type":"message","role":"assistant","model":"claude-haiku-4-5","content":[{"type":"text","text":"hello world"}],"stop_reason":"end_turn","usage":{"input_tokens":42,"output_tokens":7}}"#)
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
    async fn empty_content_array_errors() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1/messages")
            .with_status(200)
            .with_body(r#"{"content":[]}"#)
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
                "model": "claude-sonnet-4-6"
            })))
            .with_status(200)
            .with_body(r#"{"content":[{"type":"text","text":"ok"}]}"#)
            .create_async()
            .await;
        let backend = AnthropicBackend::new("k".into()).with_base_url(server.url());
        let mut p = sample_prompt();
        p.model = Some("claude-sonnet-4-6".into());
        backend.complete(&p).await.unwrap();
        mock.assert_async().await;
    }
}
