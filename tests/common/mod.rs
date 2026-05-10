//! Shared scaffolding for end-to-end tests.
//!
//! Each `tests/e2e_*.rs` integration test pulls this in via `mod common;`.
//! [`TestEnv`] sets up an isolated tempdir that doubles as `$HOME`, writes
//! bundled prompts + a config + a transcript, and hands you a pre-configured
//! `Command` that runs the alluvium binary against this fake home.
//!
//! LLM calls are short-circuited via `ALLUVIUM_FAKE_LLM_RESPONSE_PATH` —
//! the binary reads canned JSON from the fixture you write with
//! [`TestEnv::write_fake_response`] instead of hitting a real provider.

#![allow(dead_code)]

use assert_cmd::Command;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct TestEnv {
    pub home: TempDir,
    pub vault_path: PathBuf,
    pub session_id: String,
    pub transcript_path: PathBuf,
    pub fake_response_path: PathBuf,
}

impl TestEnv {
    pub fn new(session_id: &str) -> Self {
        Self::new_with_skip_paths(session_id, &[])
    }

    /// Like [`new`] but seeds `[default].skip_paths` in the generated
    /// config so tests can exercise the self-reference filter.
    pub fn new_with_skip_paths(session_id: &str, skip_paths: &[&str]) -> Self {
        let home = tempfile::tempdir().expect("tempdir for test home");
        let home_path = home.path();

        let vault = home_path.join("vault");
        std::fs::create_dir_all(&vault).expect("create vault dir");

        // Mirror what `directories::ProjectDirs::from("dev","alluvium","alluvium")`
        // resolves to once we point HOME at our tempdir. The two layouts are
        // the only platforms our hooks target.
        let config_dir = binary_config_dir(home_path);
        std::fs::create_dir_all(&config_dir).expect("create config dir");

        // Minimal config — `backend` left unset (the env-var fake backend
        // shortcuts selection before auto-detect runs anyway).
        let mut config_toml = format!(
            "[default]\nvault_path = \"{}\"\nrecipe = \"dev-journal\"\n",
            vault.display()
        );
        if !skip_paths.is_empty() {
            config_toml.push_str("skip_paths = [");
            for (i, p) in skip_paths.iter().enumerate() {
                if i > 0 {
                    config_toml.push_str(", ");
                }
                config_toml.push_str(&format!("\"{p}\""));
            }
            config_toml.push_str("]\n");
        }
        std::fs::write(config_dir.join("config.toml"), config_toml).expect("write config.toml");

        // Bundled prompts must be on disk for `distiller::prompt::load`
        // (tests don't run `alluvium init`).
        let prompts_dir = config_dir.join("prompts");
        alluvium::cli::paths::install_bundled_prompts(&prompts_dir)
            .expect("install bundled prompts");

        // Transcript at the path archive's manual-mode lookup expects:
        // `$HOME/.claude/projects/<some-name>/<session_id>.jsonl`.
        let projects = home_path
            .join(".claude")
            .join("projects")
            .join("test-project");
        std::fs::create_dir_all(&projects).expect("create transcripts dir");
        let transcript_path = projects.join(format!("{session_id}.jsonl"));
        std::fs::write(&transcript_path, sample_transcript(session_id))
            .expect("write transcript jsonl");

        let fake_response_path = home_path.join("fake_response.json");

        Self {
            home,
            vault_path: vault,
            session_id: session_id.to_string(),
            transcript_path,
            fake_response_path,
        }
    }

    /// Write the JSON the FakeBackend will return as the LLM response.
    pub fn write_fake_response(&self, json: &str) {
        std::fs::write(&self.fake_response_path, json).expect("write fake response");
    }

    /// Pre-create a topic page (used by `archive_update` and `user_edit_preserved`
    /// to seed an existing file before running the binary).
    pub fn write_existing_page(&self, rel_path: &str, content: &str) {
        let path = self.alluvium_subdir().join(rel_path);
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir for existing page");
        std::fs::write(&path, content).expect("write existing page");
    }

    /// Create an additional transcript at the same projects dir as the
    /// primary session. Used by tests that need multiple sessions
    /// (e.g. replay_bulk, concurrent_sessions).
    pub fn add_transcript(&self, session_id: &str) -> std::path::PathBuf {
        let projects = self.transcript_path.parent().unwrap();
        let path = projects.join(format!("{session_id}.jsonl"));
        std::fs::write(&path, sample_transcript(session_id)).expect("write extra transcript");
        path
    }

    /// Path to the binary's config.toml inside this fake home.
    pub fn config_path(&self) -> PathBuf {
        binary_config_dir(self.home.path()).join("config.toml")
    }

    /// Rewrite `[default].skip_paths` in the config. Used by tests that
    /// can only choose the skip dir AFTER TestEnv has created the tempdir
    /// (e.g. self_reference, where the skip path is a sub-dir of HOME).
    pub fn set_skip_paths(&self, paths: &[&Path]) {
        let mut toml = format!(
            "[default]\nvault_path = \"{}\"\nrecipe = \"dev-journal\"\n",
            self.vault_path.display()
        );
        if !paths.is_empty() {
            toml.push_str("skip_paths = [");
            for (i, p) in paths.iter().enumerate() {
                if i > 0 {
                    toml.push_str(", ");
                }
                toml.push_str(&format!("\"{}\"", p.display()));
            }
            toml.push_str("]\n");
        }
        std::fs::write(self.config_path(), toml).expect("rewrite config.toml");
    }

    /// JSON payload Claude Code would feed to a hook on stdin. Used by
    /// tests that drive `alluvium archive --detached` / `alluvium pre-compact`
    /// directly instead of via `--session`.
    pub fn hook_payload_json(&self, event: &str) -> String {
        format!(
            r#"{{"session_id":"{sid}","transcript_path":"{tp}","cwd":"{cwd}","hook_event_name":"{event}"}}"#,
            sid = self.session_id,
            tp = self.transcript_path.display(),
            cwd = self.home.path().display(),
            event = event,
        )
    }

    /// Build an `assert_cmd::Command` for the alluvium binary with HOME
    /// pointed at the tempdir and the fake-LLM env var set. Use this for
    /// `.assert().success()` style tests.
    pub fn alluvium(&self) -> Command {
        let mut cmd = Command::cargo_bin("alluvium").expect("alluvium binary built");
        cmd.env_clear();
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        cmd.env("HOME", self.home.path());
        cmd.env("XDG_CONFIG_HOME", self.home.path().join(".config"));
        cmd.env("XDG_CACHE_HOME", self.home.path().join(".cache"));
        cmd.env(
            "XDG_DATA_HOME",
            self.home.path().join(".local").join("share"),
        );
        cmd.env("ALLUVIUM_FAKE_LLM_RESPONSE_PATH", &self.fake_response_path);
        cmd
    }

    /// Same as [`alluvium`] but returns `std::process::Command`. Use this
    /// when you need to drive stdin / wait / spawn manually (e.g. piping a
    /// hook payload, measuring exit time).
    pub fn alluvium_std(&self) -> std::process::Command {
        let exe = assert_cmd::cargo::cargo_bin("alluvium");
        let mut cmd = std::process::Command::new(exe);
        cmd.env_clear();
        if let Ok(path) = std::env::var("PATH") {
            cmd.env("PATH", path);
        }
        cmd.env("HOME", self.home.path());
        cmd.env("XDG_CONFIG_HOME", self.home.path().join(".config"));
        cmd.env("XDG_CACHE_HOME", self.home.path().join(".cache"));
        cmd.env(
            "XDG_DATA_HOME",
            self.home.path().join(".local").join("share"),
        );
        cmd.env("ALLUVIUM_FAKE_LLM_RESPONSE_PATH", &self.fake_response_path);
        cmd
    }

    pub fn alluvium_subdir(&self) -> PathBuf {
        self.vault_path.join("Alluvium")
    }
}

fn binary_config_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library")
            .join("Application Support")
            .join("dev.alluvium.alluvium")
    } else {
        home.join(".config").join("alluvium")
    }
}

fn sample_transcript(session_id: &str) -> String {
    // Two events: a user prompt + an assistant reply. Enough for
    // reconstruct + metadata to produce non-empty conversation data.
    format!(
        r#"{{"type":"user","sessionId":"{session_id}","uuid":"u1","timestamp":"2026-05-10T10:00:00.000Z","cwd":"/tmp/test","message":{{"role":"user","content":"Let's design Alluvium."}}}}
{{"type":"assistant","sessionId":"{session_id}","uuid":"a1","parentUuid":"u1","timestamp":"2026-05-10T10:00:01.000Z","cwd":"/tmp/test","message":{{"role":"assistant","content":[{{"type":"text","text":"Sure — Alluvium auto-archives Claude Code sessions to a Karpathy-style wiki."}}]}}}}
"#
    )
}
