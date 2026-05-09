//! Generate and validate `.claude-plugin/plugin.json`.
//!
//! Used by `alluvium init` to install Alluvium as a Claude Code plugin
//! (rather than mutating the user's `~/.claude/settings.json`). See
//! ADR-004.
//!
//! ## v0.1 scope
//!
//! - Build the plugin manifest as a structured value (so we can
//!   programmatically verify it's well-formed before printing install
//!   instructions to the user).
//! - Validate an existing manifest matches our expected hooks.
//! - Print install instructions; do NOT auto-write to the user's
//!   `~/.claude/plugins/` because the exact convention there is still in
//!   flux. Telling the user `claude plugin install <repo>` is more robust
//!   than trying to copy files into a directory whose layout might
//!   change between Claude Code releases.
//!
//! v0.2 will add `alluvium plugin install` that does the file move once
//! we've verified the directory convention.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const PLUGIN_NAME: &str = "alluvium";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Minimal manifest shape we produce + verify.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    pub hooks: Vec<HookEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookEntry {
    pub event: String,
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Build the canonical manifest for this build of Alluvium.
pub fn build_manifest() -> PluginManifest {
    PluginManifest {
        name: PLUGIN_NAME.into(),
        version: PLUGIN_VERSION.into(),
        description:
            "Auto-archive Claude Code sessions to your Obsidian vault as a Karpathy-style LLM wiki."
                .into(),
        author: Some("Eric".into()),
        homepage: Some("https://github.com/Tespera/alluvium".into()),
        license: Some("MIT OR Apache-2.0".into()),
        hooks: vec![
            HookEntry {
                event: "SessionStart".into(),
                command: "alluvium session-start".into(),
                description: Some(
                    "Resolve config, write per-session metadata, decide self-filter.".into(),
                ),
            },
            HookEntry {
                event: "PreCompact".into(),
                command: "alluvium pre-compact".into(),
                description: Some(
                    "Snapshot transcript before compaction so original detail is not lost.".into(),
                ),
            },
            HookEntry {
                event: "Stop".into(),
                command: "alluvium archive".into(),
                description: Some(
                    "Spawn detached archive worker; hook returns within 100ms.".into(),
                ),
            },
            HookEntry {
                event: "SessionEnd".into(),
                command: "alluvium session-end".into(),
                description: Some("Clean up per-session cache directory.".into()),
            },
        ],
    }
}

/// Serialize the manifest to a pretty-printed JSON string.
pub fn render(manifest: &PluginManifest) -> Result<String> {
    serde_json::to_string_pretty(manifest).context("serializing plugin manifest")
}

/// Parse a manifest from a JSON string.
pub fn parse(content: &str) -> Result<PluginManifest> {
    serde_json::from_str(content).context("parsing plugin manifest JSON")
}

/// Validate that an existing on-disk manifest is structurally correct and
/// declares the expected hooks. Returns `Ok(())` if good, otherwise an
/// error describing what's missing / different.
pub fn validate(content: &str) -> Result<()> {
    let m = parse(content)?;
    if m.name != PLUGIN_NAME {
        anyhow::bail!("manifest name is {:?}, expected {PLUGIN_NAME:?}", m.name);
    }

    let expected_events = ["SessionStart", "PreCompact", "Stop", "SessionEnd"];
    for ev in &expected_events {
        if !m.hooks.iter().any(|h| h.event == *ev) {
            anyhow::bail!("manifest is missing required hook event {ev:?}");
        }
    }
    Ok(())
}

/// Print the install instruction the user should run to register this
/// plugin with Claude Code. The path is the directory containing
/// `.claude-plugin/plugin.json` (typically the repo root or the bundled
/// resources directory after `alluvium init`).
pub fn install_instruction(plugin_root: &std::path::Path) -> String {
    format!(
        "To register the Alluvium plugin with Claude Code, run:\n\n  \
         claude plugin install {}\n\n\
         (This is a manual step in v0.1; automated install will land in v0.2.)",
        plugin_root.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_manifest_has_all_four_hooks() {
        let m = build_manifest();
        let events: Vec<&str> = m.hooks.iter().map(|h| h.event.as_str()).collect();
        assert!(events.contains(&"SessionStart"));
        assert!(events.contains(&"PreCompact"));
        assert!(events.contains(&"Stop"));
        assert!(events.contains(&"SessionEnd"));
    }

    #[test]
    fn build_manifest_uses_alluvium_subcommands() {
        let m = build_manifest();
        for h in &m.hooks {
            assert!(
                h.command.starts_with("alluvium "),
                "hook command should start with 'alluvium '; got {:?}",
                h.command
            );
        }
    }

    #[test]
    fn build_manifest_stop_hook_does_not_use_session_id_env() {
        // Sanity check against the early bug (ADR-011): Stop hook must NOT
        // reference $SESSION_ID. The archive subcommand reads stdin instead.
        let m = build_manifest();
        let stop = m.hooks.iter().find(|h| h.event == "Stop").unwrap();
        assert!(
            !stop.command.contains("$SESSION_ID") && !stop.command.contains("--session"),
            "Stop hook must not pass --session; archive reads stdin payload"
        );
    }

    #[test]
    fn render_round_trips_through_parse() {
        let m1 = build_manifest();
        let json = render(&m1).unwrap();
        let m2 = parse(&json).unwrap();
        assert_eq!(m1, m2);
    }

    #[test]
    fn validate_accepts_canonical_manifest() {
        let m = build_manifest();
        let json = render(&m).unwrap();
        validate(&json).unwrap();
    }

    #[test]
    fn validate_rejects_missing_hook() {
        let mut m = build_manifest();
        m.hooks.retain(|h| h.event != "PreCompact");
        let json = render(&m).unwrap();
        let err = validate(&json).unwrap_err();
        assert!(format!("{err:#}").contains("PreCompact"));
    }

    #[test]
    fn validate_rejects_wrong_name() {
        let mut m = build_manifest();
        m.name = "imposter".into();
        let json = render(&m).unwrap();
        let err = validate(&json).unwrap_err();
        assert!(format!("{err:#}").contains("imposter"));
    }

    #[test]
    fn validate_rejects_malformed_json() {
        let err = validate("this is not json").unwrap_err();
        assert!(format!("{err:#}").contains("parsing plugin manifest"));
    }

    #[test]
    fn install_instruction_includes_path() {
        let s = install_instruction(std::path::Path::new("/some/plugin/root"));
        assert!(s.contains("/some/plugin/root"));
        assert!(s.contains("claude plugin install"));
    }

    /// Smoke: the on-disk `.claude-plugin/plugin.json` should validate.
    #[test]
    fn shipped_plugin_json_validates() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".claude-plugin")
            .join("plugin.json");
        if !path.exists() {
            return; // unusual but don't fail the suite
        }
        let content = std::fs::read_to_string(&path).unwrap();
        validate(&content).expect("shipped plugin.json must validate");
    }
}
