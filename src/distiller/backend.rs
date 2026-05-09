//! LLM backend abstraction.
//!
//! v0.1 supports five backends (auto-detected, or explicitly chosen via
//! `[default] backend = "..."` in config.toml):
//!
//! | kind          | auth                                   | model default       |
//! | ------------- | -------------------------------------- | ------------------- |
//! | `claude-cli`  | user's Claude Code OAuth (no key)      | uses CC default     |
//! | `anthropic`   | `$ANTHROPIC_API_KEY` or OS keychain    | claude-haiku-4-5    |
//! | `openai`      | `$OPENAI_API_KEY`                      | gpt-4o-mini         |
//! | `deepseek`    | `$DEEPSEEK_API_KEY`                    | deepseek-chat       |
//! | `gemini`      | `$GEMINI_API_KEY`                      | gemini-1.5-flash    |
//!
//! Auto-detect order (when `backend` not in config): claude-cli (if `claude`
//! is on PATH) → anthropic (if env var) → openai → deepseek → gemini → error.

use anyhow::Result;

use super::TokenUsage;

/// Backend-agnostic prompt to render to whichever LLM is selected.
#[derive(Debug, Clone)]
pub struct RenderedPrompt {
    /// System / instruction prompt.
    pub system: String,
    /// User-side prompt content (the rendered jinja template).
    pub user: String,
    /// Optional model override (otherwise backend uses its default).
    pub model: Option<String>,
    /// Soft cap on response token count.
    pub max_tokens: u32,
}

/// Generic LLM call result.
#[derive(Debug, Clone)]
pub struct LlmResponse {
    /// The model's output text. Should be JSON conforming to ADR-013 if the
    /// system prompt said so; downstream parser is tolerant of code-fence
    /// wrapping and surrounding prose.
    pub text: String,
    /// Token usage if the backend reports it. `claude -p` does not surface
    /// usage in plain-text output mode; it'll be `None` for that backend.
    pub usage: Option<TokenUsage>,
}

/// One LLM backend.
#[async_trait::async_trait]
pub trait LlmBackend: Send + Sync {
    async fn complete(&self, prompt: &RenderedPrompt) -> Result<LlmResponse>;

    /// Human-readable name used in logs ("claude-cli", "anthropic", "openai", "deepseek", "gemini").
    fn kind(&self) -> &'static str;
}
