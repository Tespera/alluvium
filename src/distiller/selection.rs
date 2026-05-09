//! Backend selection: explicit config override, otherwise auto-detect.
//!
//! Auto-detection order:
//!   1. `claude` on PATH        → ClaudeCliBackend
//!   2. `$ANTHROPIC_API_KEY`    → AnthropicBackend
//!   3. `$OPENAI_API_KEY`       → OpenAiCompatibleBackend (openai)
//!   4. `$DEEPSEEK_API_KEY`     → OpenAiCompatibleBackend (deepseek)
//!   5. `$GEMINI_API_KEY`       → GeminiBackend
//!   6. fall-through            → error "no backend available"
//!
//! Note: when `backend = "anthropic"` is explicit, we ALSO consult the OS
//! Keychain (via `config::secrets::get_api_key`), because that's the path
//! `alluvium init` writes into. For all other backends, env var only —
//! they're rarer and adding keychain support per-provider can come later.

use anyhow::{Context, Result};
use std::sync::Arc;

use super::backend::LlmBackend;
use super::backends::{
    anthropic::AnthropicBackend, claude_cli::ClaudeCliBackend, gemini::GeminiBackend,
    openai_compatible::OpenAiCompatibleBackend,
};

/// What backend to use, as named in `config.toml`'s `[default] backend = "..."`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    ClaudeCli,
    Anthropic,
    OpenAi,
    DeepSeek,
    Gemini,
}

impl BackendKind {
    pub fn from_config_str(s: &str) -> Option<Self> {
        match s {
            "claude-cli" | "claude" => Some(Self::ClaudeCli),
            "anthropic" => Some(Self::Anthropic),
            "openai" => Some(Self::OpenAi),
            "deepseek" => Some(Self::DeepSeek),
            "gemini" => Some(Self::Gemini),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ClaudeCli => "claude-cli",
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::DeepSeek => "deepseek",
            Self::Gemini => "gemini",
        }
    }
}

/// Choose a backend based on config (`backend_config_str`, `model_config_str`),
/// falling back to auto-detection if config doesn't specify one.
///
/// `model_config_str` is the optional user-set model name; backends apply it
/// as an override, otherwise use their own default.
pub fn pick(
    backend_config_str: Option<&str>,
    model_config_str: Option<&str>,
) -> Result<Arc<dyn LlmBackend>> {
    let kind = match backend_config_str {
        Some(s) => BackendKind::from_config_str(s)
            .ok_or_else(|| anyhow::anyhow!("unknown backend kind: {s:?}; valid: claude-cli, anthropic, openai, deepseek, gemini"))?,
        None => auto_detect()?,
    };

    construct(kind, model_config_str)
}

/// Try each backend in priority order; return the first one whose preconditions
/// (binary on PATH or env var set) are met.
fn auto_detect() -> Result<BackendKind> {
    if claude_cli_available() {
        return Ok(BackendKind::ClaudeCli);
    }
    if env_set("ANTHROPIC_API_KEY") {
        return Ok(BackendKind::Anthropic);
    }
    if env_set("OPENAI_API_KEY") {
        return Ok(BackendKind::OpenAi);
    }
    if env_set("DEEPSEEK_API_KEY") {
        return Ok(BackendKind::DeepSeek);
    }
    if env_set("GEMINI_API_KEY") {
        return Ok(BackendKind::Gemini);
    }
    anyhow::bail!(
        "no LLM backend available. Install Claude Code (`claude` on PATH) or \
         set one of: ANTHROPIC_API_KEY, OPENAI_API_KEY, DEEPSEEK_API_KEY, GEMINI_API_KEY"
    )
}

fn claude_cli_available() -> bool {
    which::which("claude").is_ok()
}

fn env_set(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty())
}

fn construct(kind: BackendKind, model: Option<&str>) -> Result<Arc<dyn LlmBackend>> {
    match kind {
        BackendKind::ClaudeCli => {
            let mut b = ClaudeCliBackend::new();
            if let Some(m) = model {
                b = b.with_default_model(m);
            }
            Ok(Arc::new(b))
        }
        BackendKind::Anthropic => {
            // Prefer env var; fall back to OS keychain (where `alluvium init` stores).
            let key = anthropic_key()?;
            let mut b = AnthropicBackend::new(key);
            if let Some(m) = model {
                b = b.with_default_model(m);
            }
            Ok(Arc::new(b))
        }
        BackendKind::OpenAi => {
            let key = require_env("OPENAI_API_KEY")?;
            let mut b = OpenAiCompatibleBackend::openai(key);
            if let Some(m) = model {
                b = b.with_default_model(m);
            }
            Ok(Arc::new(b))
        }
        BackendKind::DeepSeek => {
            let key = require_env("DEEPSEEK_API_KEY")?;
            let mut b = OpenAiCompatibleBackend::deepseek(key);
            if let Some(m) = model {
                b = b.with_default_model(m);
            }
            Ok(Arc::new(b))
        }
        BackendKind::Gemini => {
            let key = require_env("GEMINI_API_KEY")?;
            let mut b = GeminiBackend::new(key);
            if let Some(m) = model {
                b = b.with_default_model(m);
            }
            Ok(Arc::new(b))
        }
    }
}

fn anthropic_key() -> Result<String> {
    if let Ok(k) = std::env::var("ANTHROPIC_API_KEY") {
        if !k.is_empty() {
            return Ok(k);
        }
    }
    crate::config::secrets::get_api_key().context(
        "anthropic backend needs ANTHROPIC_API_KEY env var or a key stored via `alluvium init`",
    )
}

fn require_env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("{name} env var not set"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_backend_kind_aliases() {
        assert_eq!(
            BackendKind::from_config_str("claude-cli"),
            Some(BackendKind::ClaudeCli)
        );
        assert_eq!(
            BackendKind::from_config_str("claude"),
            Some(BackendKind::ClaudeCli)
        );
        assert_eq!(
            BackendKind::from_config_str("anthropic"),
            Some(BackendKind::Anthropic)
        );
        assert_eq!(
            BackendKind::from_config_str("openai"),
            Some(BackendKind::OpenAi)
        );
        assert_eq!(
            BackendKind::from_config_str("deepseek"),
            Some(BackendKind::DeepSeek)
        );
        assert_eq!(
            BackendKind::from_config_str("gemini"),
            Some(BackendKind::Gemini)
        );
        assert_eq!(BackendKind::from_config_str("unknown"), None);
    }

    #[test]
    fn unknown_backend_in_config_errors() {
        // pick returns Result<Arc<dyn LlmBackend>>; LlmBackend lacks Debug,
        // so we can't .unwrap_err() directly. Match instead.
        match pick(Some("not-a-real-backend"), None) {
            Ok(_) => panic!("expected error for unknown backend"),
            Err(err) => assert!(format!("{err:#}").contains("unknown backend")),
        }
    }

    #[test]
    fn label_matches_config_string() {
        assert_eq!(BackendKind::ClaudeCli.label(), "claude-cli");
        assert_eq!(BackendKind::Anthropic.label(), "anthropic");
    }
}
