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
use std::time::Duration;
use tokio::process::Command;

use crate::distiller::backend::{LlmBackend, LlmResponse, RenderedPrompt};

/// Hard ceiling on how long we wait for `claude -p` to return. Empirically
/// successful runs finish in 5-30 s; observed pathology is a hang of several
/// minutes that ends with truncated stdout and exit 0 — masquerading as a
/// successful call. Capping at 3 min turns that into a clear timeout error
/// instead of garbage propagating into the parser.
///
/// Override with `ALLUVIUM_CLAUDE_CLI_TIMEOUT_SECS` (undocumented, advanced).
const DEFAULT_TIMEOUT_SECS: u64 = 180;
const TIMEOUT_OVERRIDE_ENV: &str = "ALLUVIUM_CLAUDE_CLI_TIMEOUT_SECS";

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
            .stderr(Stdio::piped())
            // If we drop the child handle (e.g. on timeout), SIGKILL it.
            // Otherwise the orphaned `claude -p` would keep tying up an API
            // session in the background.
            .kill_on_drop(true);

        let child = cmd
            .spawn()
            .with_context(|| format!("spawning {} -p", self.binary.display()))?;

        let timeout = resolve_timeout();
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => {
                return Err(e).with_context(|| format!("waiting on {} -p", self.binary.display()))
            }
            Err(_) => {
                anyhow::bail!(
                    "claude -p timed out after {}s — likely a hung CLI call. \
                     Investigate or override with {TIMEOUT_OVERRIDE_ENV}.",
                    timeout.as_secs()
                );
            }
        };

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

fn resolve_timeout() -> Duration {
    let secs = std::env::var(TIMEOUT_OVERRIDE_ENV)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    Duration::from_secs(secs)
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

    #[tokio::test]
    async fn slow_binary_is_killed_after_timeout() {
        // Drop a small shell script that ignores its args and sleeps 60 s.
        // The script substitutes for a "hung" claude -p so we can observe
        // that the timeout path fires and reports a clean error.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("hung_claude.sh");
        std::fs::write(&script, "#!/bin/sh\nsleep 60\n").unwrap();
        let mut perms = std::fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&script, perms).unwrap();

        // Shrink the deadline so the test doesn't actually wait minutes.
        std::env::set_var(TIMEOUT_OVERRIDE_ENV, "1");

        let backend = ClaudeCliBackend::new().with_binary(script);
        let start = std::time::Instant::now();
        let err = backend.complete(&sample_prompt()).await.unwrap_err();
        let elapsed = start.elapsed();

        std::env::remove_var(TIMEOUT_OVERRIDE_ENV);

        let msg = format!("{err:#}");
        assert!(
            msg.contains("timed out"),
            "expected timeout error, got: {msg}"
        );
        // Generous slack — CI machines vary — but it must NOT have waited
        // the full 60 s the script was sleeping for.
        assert!(
            elapsed.as_secs() < 10,
            "timeout fired but took too long: {elapsed:?}"
        );
    }
}
