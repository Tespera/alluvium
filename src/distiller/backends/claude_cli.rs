//! `claude -p` (Claude Code non-interactive) backend.
//!
//! Spawns the `claude` CLI in print mode using the user's existing Claude
//! Code authentication. No separate API key required — the user's
//! Claude Code subscription / OAuth handles auth.
//!
//! ## Recursion prevention
//!
//! When this backend runs from inside an Alluvium archive (which itself was
//! triggered by a Claude Code Stop hook), the spawned `claude -p` is a
//! short-lived session. When that session ends, its own Stop hook fires —
//! and that hook is `alluvium archive`, which would call `claude -p` again,
//! and so on infinitely.
//!
//! We break the cycle by setting `ALLUVIUM_DISTILLING=1` in the spawned
//! claude's environment. Alluvium's hook handlers check this env var on
//! entry and exit early when set. See [`crate::cli::session_start`] etc.
//!
//! ## Why not `--bare`
//!
//! `claude --bare` would disable hooks (and thus prevent recursion
//! cleanly), but also REQUIRES `$ANTHROPIC_API_KEY` (it disables OAuth /
//! keychain). That defeats the whole point of the claude-cli backend. So
//! we use the env-var-based recursion guard instead.

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

use crate::distiller::backend::{LlmBackend, LlmResponse, RenderedPrompt};

#[derive(Debug, Clone)]
pub struct ClaudeCliBackend {
    binary: PathBuf,
    /// Optional `--model` override. `None` lets Claude Code pick its default.
    default_model: Option<String>,
}

impl ClaudeCliBackend {
    /// Construct using `claude` from `$PATH`.
    pub fn new() -> Self {
        Self {
            binary: PathBuf::from("claude"),
            default_model: None,
        }
    }

    /// For tests: override the binary path.
    pub fn with_binary(mut self, binary: PathBuf) -> Self {
        self.binary = binary;
        self
    }

    pub fn with_default_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = Some(model.into());
        self
    }
}

impl Default for ClaudeCliBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl LlmBackend for ClaudeCliBackend {
    fn kind(&self) -> &'static str {
        "claude-cli"
    }

    async fn complete(&self, prompt: &RenderedPrompt) -> Result<LlmResponse> {
        let mut cmd = Command::new(&self.binary);
        cmd.arg("-p")
            .arg("--no-session-persistence")
            .arg("--system-prompt")
            .arg(&prompt.system);

        let model = prompt.model.as_deref().or(self.default_model.as_deref());
        if let Some(m) = model {
            cmd.arg("--model").arg(m);
        }

        cmd.arg(&prompt.user)
            .env("ALLUVIUM_DISTILLING", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let output = cmd
            .output()
            .await
            .with_context(|| format!("running {} -p", self.binary.display()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let preview: String = stderr.chars().take(500).collect();
            anyhow::bail!("claude -p exited with {}: {preview}", output.status);
        }

        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            anyhow::bail!("claude -p produced no output");
        }

        Ok(LlmResponse {
            text,
            // claude -p text mode does not expose token counts.
            usage: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_prompt() -> RenderedPrompt {
        RenderedPrompt {
            system: "SYS".into(),
            user: "USER".into(),
            model: None,
            max_tokens: 1024,
        }
    }

    /// Smoke test: pass `/bin/echo` as the binary so we can verify the
    /// subprocess plumbing without invoking real claude.
    #[tokio::test]
    async fn echo_binary_returns_stdout_text() {
        let backend = ClaudeCliBackend::new().with_binary(PathBuf::from("/bin/echo"));
        let r = backend.complete(&sample_prompt()).await.unwrap();
        // /bin/echo will print all its args separated by spaces. We just verify
        // it didn't error and that some output came back.
        assert!(!r.text.is_empty());
        assert!(r.usage.is_none());
    }

    #[tokio::test]
    async fn nonexistent_binary_errors() {
        let backend = ClaudeCliBackend::new().with_binary(PathBuf::from("/does/not/exist/claude"));
        let err = backend.complete(&sample_prompt()).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("running") || msg.contains("/does/not/exist"));
    }

    #[tokio::test]
    async fn nonzero_exit_propagates_stderr() {
        // /usr/bin/false exits 1 with no output — we should surface the exit code.
        let backend = ClaudeCliBackend::new().with_binary(PathBuf::from("/usr/bin/false"));
        let err = backend.complete(&sample_prompt()).await.unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("exited"), "got: {msg}");
    }

    #[test]
    fn kind_is_claude_cli() {
        assert_eq!(ClaudeCliBackend::new().kind(), "claude-cli");
    }
}
