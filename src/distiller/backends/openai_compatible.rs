//! OpenAI / DeepSeek backend.
//!
//! Both providers expose the same Chat Completions API shape (DeepSeek is
//! deliberately OpenAI-compatible). Differences are handled via:
//! - `base_url`: openai → `api.openai.com/v1`, deepseek → `api.deepseek.com`
//! - default model
//! - which env var the API key comes from (caller's job; we just take the
//!   key as a String)
//!
//! Both rely on `response_format: {"type": "json_object"}` to coax JSON
//! output from the model. The system prompt also explicitly instructs JSON.

use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::distiller::backend::{LlmBackend, LlmResponse, RenderedPrompt};
use crate::distiller::TokenUsage;

#[derive(Debug, Clone)]
pub struct OpenAiCompatibleBackend {
    api_key: String,
    base_url: String,
    default_model: String,
    /// Identifier for logs ("openai" or "deepseek").
    flavor: &'static str,
    http: reqwest::Client,
}

impl OpenAiCompatibleBackend {
    pub fn openai(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.openai.com/v1".into(),
            default_model: "gpt-4o-mini".into(),
            flavor: "openai",
            http: reqwest::Client::new(),
        }
    }

    pub fn deepseek(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.deepseek.com".into(),
            default_model: "deepseek-chat".into(),
            flavor: "deepseek",
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
impl LlmBackend for OpenAiCompatibleBackend {
    fn kind(&self) -> &'static str {
        self.flavor
    }

    async fn complete(&self, prompt: &RenderedPrompt) -> Result<LlmResponse> {
        let model = prompt.model.as_deref().unwrap_or(&self.default_model);
        let url = format!("{}/chat/completions", self.base_url);
        let body = serde_json::json!({
            "model": model,
            "max_tokens": prompt.max_tokens,
            "messages": [
                {"role": "system", "content": prompt.system},
                {"role": "user", "content": prompt.user}
            ],
            "response_format": {"type": "json_object"},
        });

        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .with_context(|| format!("HTTP request to {url} failed"))?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .context("reading OpenAI-compatible response body")?;

        if !status.is_success() {
            let preview: String = body_text.chars().take(500).collect();
            anyhow::bail!("{} API returned {status}: {preview}", self.flavor);
        }

        let parsed: serde_json::Value = serde_json::from_str(&body_text).with_context(|| {
            let preview: String = body_text.chars().take(500).collect();
            format!(
                "{} response was not valid JSON; first 500 chars: {preview}",
                self.flavor
            )
        })?;

        let text = parsed
            .get("choices")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|m| m.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "{} response missing choices[0].message.content",
                    self.flavor
                )
            })?
            .to_string();

        let usage = parsed.get("usage").and_then(|u| {
            let input = u.get("prompt_tokens")?.as_u64()?;
            let output = u.get("completion_tokens")?.as_u64()?;
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
            system: "SYS".into(),
            user: "USER".into(),
            model: None,
            max_tokens: 1024,
        }
    }

    #[tokio::test]
    async fn openai_happy_path() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"role":"assistant","content":"{\"hello\":\"world\"}"}}],"usage":{"prompt_tokens":11,"completion_tokens":3}}"#)
            .create_async()
            .await;
        let backend = OpenAiCompatibleBackend::openai("k".into()).with_base_url(server.url());
        let r = backend.complete(&sample_prompt()).await.unwrap();
        assert_eq!(r.text, r#"{"hello":"world"}"#);
        let u = r.usage.unwrap();
        assert_eq!(u.input_tokens, 11);
        assert_eq!(u.output_tokens, 3);
    }

    #[tokio::test]
    async fn deepseek_uses_separate_kind_label() {
        let backend = OpenAiCompatibleBackend::deepseek("k".into());
        assert_eq!(backend.kind(), "deepseek");
        let backend = OpenAiCompatibleBackend::openai("k".into());
        assert_eq!(backend.kind(), "openai");
    }

    #[tokio::test]
    async fn missing_choices_array_errors() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(200)
            .with_body(r#"{}"#)
            .create_async()
            .await;
        let backend = OpenAiCompatibleBackend::openai("k".into()).with_base_url(server.url());
        let err = backend.complete(&sample_prompt()).await.unwrap_err();
        assert!(format!("{err:#}").contains("missing choices"));
    }

    #[tokio::test]
    async fn http_error_includes_status() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/chat/completions")
            .with_status(429)
            .with_body(r#"{"error":"rate limited"}"#)
            .create_async()
            .await;
        let backend = OpenAiCompatibleBackend::openai("k".into()).with_base_url(server.url());
        let err = backend.complete(&sample_prompt()).await.unwrap_err();
        assert!(format!("{err:#}").contains("429"));
    }

    #[tokio::test]
    async fn request_includes_response_format_json_object() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/chat/completions")
            .match_body(mockito::Matcher::PartialJson(serde_json::json!({
                "response_format": {"type": "json_object"}
            })))
            .with_status(200)
            .with_body(r#"{"choices":[{"message":{"content":"{}"}}]}"#)
            .create_async()
            .await;
        let backend = OpenAiCompatibleBackend::openai("k".into()).with_base_url(server.url());
        backend.complete(&sample_prompt()).await.unwrap();
        mock.assert_async().await;
    }
}
