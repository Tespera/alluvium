//! Generate and validate `.claude-plugin/plugin.json`.
//!
//! Schema matches the official Claude Code plugin manifest format
//! (verified against `code.claude.com/docs/en/plugins-reference`):
//!
//! ```json
//! {
//!   "name": "alluvium",
//!   "author": { "name": "Eric" },
//!   "hooks": {
//!     "SessionStart": [
//!       { "hooks": [{ "type": "command", "command": "alluvium session-start" }] }
//!     ],
//!     ...
//!   }
//! }
//! ```
//!
//! Hooks are grouped by event name → list of "hook groups" (each may have an
//! optional `matcher`) → list of typed `{type, command}` entries. Most
//! plugins use a single group with no matcher, but the schema supports
//! tool-name matching for `PostToolUse` etc.
//!
//! ## v0.1 scope
//!
//! - Build the canonical manifest (tested as round-trippable).
//! - Validate an existing manifest matches our expected hook events.
//! - Print install instruction. Auto-install via `claude plugin install
//!   alluvium@alluvium` is documented but not invoked from this module —
//!   the user runs that themselves after `alluvium init`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PLUGIN_NAME: &str = "alluvium";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: Option<Author>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// Map of event name → list of hook groups. Use `BTreeMap` for stable
    /// serialization order.
    #[serde(default)]
    pub hooks: BTreeMap<String, Vec<HookGroup>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Author {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookGroup {
    /// Optional tool-name matcher (used for PostToolUse / PreToolUse).
    /// Most lifecycle events (SessionStart / Stop / etc.) don't need it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<String>,
    pub hooks: Vec<HookEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookEntry {
    #[serde(rename = "type")]
    pub kind: String,
    pub command: String,
}

fn cmd(command: &str) -> HookGroup {
    HookGroup {
        matcher: None,
        hooks: vec![HookEntry {
            kind: "command".into(),
            command: command.into(),
        }],
    }
}

/// Build the canonical Alluvium manifest for this build.
pub fn build_manifest() -> PluginManifest {
    let mut hooks = BTreeMap::new();
    hooks.insert("SessionStart".into(), vec![cmd("alluvium session-start")]);
    hooks.insert("PreCompact".into(), vec![cmd("alluvium pre-compact")]);
    hooks.insert("Stop".into(), vec![cmd("alluvium archive")]);
    hooks.insert("SessionEnd".into(), vec![cmd("alluvium session-end")]);

    PluginManifest {
        name: PLUGIN_NAME.into(),
        version: PLUGIN_VERSION.into(),
        description:
            "Auto-archive Claude Code sessions to your Obsidian vault as a Karpathy-style LLM wiki."
                .into(),
        author: Some(Author {
            name: "Eric".into(),
            email: None,
            url: None,
        }),
        homepage: Some("https://github.com/Tespera/alluvium".into()),
        repository: Some("https://github.com/Tespera/alluvium".into()),
        license: Some("MIT OR Apache-2.0".into()),
        hooks,
    }
}

pub fn render(manifest: &PluginManifest) -> Result<String> {
    serde_json::to_string_pretty(manifest).context("serializing plugin manifest")
}

pub fn parse(content: &str) -> Result<PluginManifest> {
    serde_json::from_str(content).context("parsing plugin manifest JSON")
}

pub fn validate(content: &str) -> Result<()> {
    let m = parse(content)?;
    if m.name != PLUGIN_NAME {
        anyhow::bail!("manifest name is {:?}, expected {PLUGIN_NAME:?}", m.name);
    }

    let expected_events = ["SessionStart", "PreCompact", "Stop", "SessionEnd"];
    for ev in &expected_events {
        if !m.hooks.contains_key(*ev) {
            anyhow::bail!("manifest is missing required hook event {ev:?}");
        }
    }
    Ok(())
}

/// Iterate over all (event_name, command) pairs declared in the manifest.
/// Useful for validation tests and for printing install summaries.
pub fn iter_commands(m: &PluginManifest) -> impl Iterator<Item = (&str, &str)> {
    m.hooks.iter().flat_map(|(event, groups)| {
        groups.iter().flat_map(move |g| {
            g.hooks
                .iter()
                .map(move |entry| (event.as_str(), entry.command.as_str()))
        })
    })
}

pub fn install_instruction(plugin_root: &std::path::Path) -> String {
    format!(
        "To register the Alluvium plugin with Claude Code:\n\n  \
            claude plugin marketplace add {root}\n  \
            claude plugin install alluvium@alluvium\n\n\
         (`marketplace add` needs to be done once; `install` registers the plugin under your user scope.)",
        root = plugin_root.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_manifest_has_all_four_hooks() {
        let m = build_manifest();
        for ev in ["SessionStart", "PreCompact", "Stop", "SessionEnd"] {
            assert!(m.hooks.contains_key(ev), "missing event {ev}");
        }
    }

    #[test]
    fn build_manifest_uses_alluvium_subcommands() {
        let m = build_manifest();
        for (event, command) in iter_commands(&m) {
            assert!(
                command.starts_with("alluvium "),
                "hook for {event} should start with 'alluvium '; got {command:?}"
            );
        }
    }

    #[test]
    fn build_manifest_stop_hook_does_not_use_session_id_env() {
        // Regression guard against ADR-011 bug.
        let m = build_manifest();
        let stop_groups = m.hooks.get("Stop").unwrap();
        for g in stop_groups {
            for entry in &g.hooks {
                assert!(
                    !entry.command.contains("$SESSION_ID") && !entry.command.contains("--session"),
                    "Stop hook command must not pass --session; archive reads stdin payload"
                );
            }
        }
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
        m.hooks.remove("PreCompact");
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
        assert!(s.contains("claude plugin marketplace add"));
        assert!(s.contains("claude plugin install alluvium@alluvium"));
    }

    /// Smoke: the on-disk shipped manifests should validate.
    #[test]
    fn shipped_plugin_jsons_validate() {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for rel in &[
            ".claude-plugin/plugin.json",
            "plugins/alluvium/.claude-plugin/plugin.json",
        ] {
            let path = manifest_dir.join(rel);
            if !path.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&path).unwrap();
            validate(&content)
                .unwrap_or_else(|e| panic!("{} failed validation: {e:#}", path.display()));
        }
    }

    /// Author must serialize as an object, not a string.
    #[test]
    fn author_is_object_not_string() {
        let m = build_manifest();
        let json = serde_json::to_value(&m).unwrap();
        let author = json.get("author").unwrap();
        assert!(author.is_object(), "author must be object; got: {author}");
    }

    /// Hooks must serialize as a map keyed by event, not a flat array.
    #[test]
    fn hooks_is_map_keyed_by_event() {
        let m = build_manifest();
        let json = serde_json::to_value(&m).unwrap();
        let hooks = json.get("hooks").unwrap();
        assert!(hooks.is_object(), "hooks must be a map; got: {hooks}");
        assert!(hooks.get("SessionStart").is_some());
    }
}
