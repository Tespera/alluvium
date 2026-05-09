//! Google Gemini backend (`generativelanguage.googleapis.com`).
//!
//! Different schema from OpenAI / Anthropic:
//! - System prompt goes in `systemInstruction`, not the messages array.
//! - User content goes in `contents[].parts[].text`.
//! - Auth via `x-goog-api-key` header.
//! - Model name is part of the URL path: `/v1beta/models/<MODEL>:generateContent`.
//! - JSON output via `generationConfig.responseMimeType = "application/json"`.

use anyhow::{Context, Result};
use async_trait::async_trait;

use crate::distiller::backend::{LlmBackend, LlmResponse, RenderedPrompt};
use crate::distiller::TokenUsage;

const DEFAULT_BASE_URL: &str = "https://generativelanguage.googleapis.com";
const DEFAULT_MODEL: &str = "gemini-1.5-flash";

#[derive(Debug, Clone)]
pub struct GeminiBackend {
    api_key: String,
    base_url: String,
    default_model: String,
    http: reqwest::Client,
}

impl GeminiBackend {
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
impl LlmBackend for GeminiBackend {
    fn kind(&self) -> &'static str {
        "gemini"
    }

    async fn complete(&self, prompt: &RenderedPrompt) -> Result<LlmResponse> {
        let model = prompt.model.as_deref().unwrap_or(&self.default_model);
        let url = format!("{}/v1beta/models/{model}:generateContent", self.base_url);
        let body = serde_json::json!({
            "systemInstruction": {
                "parts": [{"text": prompt.system}]
            },
            "contents": [
                {"role": "user", "parts": [{"text": prompt.user}]}
            ],
            "generationConfig": {
                "responseMimeType": "application/json",
                "maxOutputTokens": prompt.max_tokens,
            }
        });

        let response = self
            .http
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .with_context(|| format!("HTTP request to {url} failed"))?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .context("reading Gemini response body")?;

        if !status.is_success() {
            let preview: String = body_text.chars().take(500).collect();
            anyhow::bail!("Gemini API returned {status}: {preview}");
        }

        let parsed: serde_json::Value = serde_json::from_str(&body_text).with_context(|| {
            let preview: String = body_text.chars().take(500).collect();
            format!("Gemini response was not valid JSON; first 500 chars: {preview}")
        })?;

        // candidates[0].content.parts[*].text — concatenate all text parts.
        let text = parsed
            .get("candidates")
            .and_then(|c| c.as_array())
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.as_array())
            .map(|parts| {
                parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(|t| t.as_str()))
                    .collect::<Vec<_>>()
                    .join("")
            })
            .ok_or_else(|| {
                anyhow::anyhow!("Gemini response missing candidates[0].content.parts")
            })?;

        if text.is_empty() {
            anyhow::bail!("Gemini response had empty text");
        }

        let usage = parsed.get("usageMetadata").and_then(|u| {
            let input = u.get("promptTokenCount")?.as_u64()?;
            let output = u.get("candidatesTokenCount")?.as_u64()?;
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
    async fn happy_path_extracts_text_and_usage() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1beta/models/gemini-1.5-flash:generateContent")
            .with_status(200)
            .with_body(r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"hello"},{"text":" world"}]}}],"usageMetadata":{"promptTokenCount":42,"candidatesTokenCount":7}}"#)
            .create_async()
            .await;
        let backend = GeminiBackend::new("k".into()).with_base_url(server.url());
        let r = backend.complete(&sample_prompt()).await.unwrap();
        assert_eq!(r.text, "hello world");
        let u = r.usage.unwrap();
        assert_eq!(u.input_tokens, 42);
        assert_eq!(u.output_tokens, 7);
    }

    #[tokio::test]
    async fn model_override_changes_url() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("POST", "/v1beta/models/gemini-1.5-pro:generateContent")
            .with_status(200)
            .with_body(r#"{"candidates":[{"content":{"parts":[{"text":"ok"}]}}]}"#)
            .create_async()
            .await;
        let backend = GeminiBackend::new("k".into()).with_base_url(server.url());
        let mut p = sample_prompt();
        p.model = Some("gemini-1.5-pro".into());
        backend.complete(&p).await.unwrap();
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn http_4xx_error_includes_status() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1beta/models/gemini-1.5-flash:generateContent")
            .with_status(403)
            .with_body(r#"{"error":{"message":"forbidden"}}"#)
            .create_async()
            .await;
        let backend = GeminiBackend::new("k".into()).with_base_url(server.url());
        let err = backend.complete(&sample_prompt()).await.unwrap_err();
        assert!(format!("{err:#}").contains("403"));
    }

    #[tokio::test]
    async fn empty_candidates_errors() {
        let mut server = Server::new_async().await;
        let _mock = server
            .mock("POST", "/v1beta/models/gemini-1.5-flash:generateContent")
            .with_status(200)
            .with_body(r#"{"candidates":[]}"#)
            .create_async()
            .await;
        let backend = GeminiBackend::new("k".into()).with_base_url(server.url());
        let err = backend.complete(&sample_prompt()).await.unwrap_err();
        assert!(format!("{err:#}").contains("missing"));
    }

    #[test]
    fn kind_is_gemini() {
        assert_eq!(GeminiBackend::new("k".into()).kind(), "gemini");
    }
}
