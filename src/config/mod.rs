//! Configuration loading & writing.
//!
//! - Reads `<config>/alluvium/config.toml` (path resolved via the
//!   `directories` crate; see ADR-010).
//! - Schema is `[default]` with optional future-reserved
//!   `[profiles.<name>]` (ADR-005 — v0.1 ignores profiles).
//! - Use [`secrets`] for the Anthropic API key (Keychain).

pub mod secrets;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Top-level config file structure.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ConfigFile {
    pub default: ProfileConfig,
    /// v0.2: profiles will live here. Loaded but not honored in v0.1.
    #[serde(default, rename = "profiles")]
    pub _reserved_profiles: std::collections::BTreeMap<String, ProfileConfig>,
}

/// Per-profile settings (v0.1 only uses `default`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileConfig {
    pub vault_path: PathBuf,
    #[serde(default = "default_subdir")]
    pub alluvium_subdir: String,
    #[serde(default = "default_recipe")]
    pub recipe: String,
    /// Model name. Optional — if absent, the chosen backend uses its own
    /// default ("claude-haiku-4-5" for anthropic, "gpt-4o-mini" for openai,
    /// etc.). For the claude-cli backend, leaving this `None` means
    /// Claude Code picks the model based on its own configuration.
    #[serde(default)]
    pub model: Option<String>,
    /// LLM backend. One of: "claude-cli" (default), "anthropic", "openai",
    /// "deepseek", "gemini". When absent, auto-detected: prefer claude-cli
    /// if `claude` is on PATH, then fall through env vars in order
    /// ANTHROPIC → OPENAI → DEEPSEEK → GEMINI.
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub keep_source_summaries: bool,
    #[serde(default)]
    pub skip_paths: Vec<PathBuf>,
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            vault_path: PathBuf::new(),
            alluvium_subdir: default_subdir(),
            recipe: default_recipe(),
            model: None,
            backend: None,
            keep_source_summaries: false,
            skip_paths: Vec::new(),
        }
    }
}

fn default_subdir() -> String {
    "Alluvium".into()
}
fn default_recipe() -> String {
    "dev-journal".into()
}

/// Resolve the canonical config file path. Returns
/// `<config_dir>/alluvium/config.toml` per the `directories` crate
/// convention, but `<config_dir>` here already includes `alluvium` (since
/// we use `ProjectDirs::from("dev", "alluvium", "alluvium")`), so the
/// final path is `<dirs.config_dir()>/config.toml`.
pub fn default_config_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "alluvium", "alluvium")
        .context("no home directory available; cannot locate config dir")?;
    Ok(dirs.config_dir().join("config.toml"))
}

/// Load a config file from `path`.
pub fn load(path: &Path) -> Result<ConfigFile> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading config file {}", path.display()))?;
    let cfg: ConfigFile = toml::from_str(&content)
        .with_context(|| format!("parsing config file {}", path.display()))?;
    Ok(cfg)
}

/// Save a config file to `path` atomically.
pub fn save(path: &Path, config: &ConfigFile) -> Result<()> {
    let content = toml::to_string_pretty(config).context("serializing config to TOML")?;
    crate::vault::writer::write_atomic(path, &content)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(path: &Path, content: &str) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn load_minimal_config_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(
            &path,
            r#"
[default]
vault_path = "/Users/eric/Vault"
"#,
        );
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.default.vault_path, PathBuf::from("/Users/eric/Vault"));
        assert_eq!(cfg.default.alluvium_subdir, "Alluvium");
        assert_eq!(cfg.default.recipe, "dev-journal");
        assert_eq!(cfg.default.model, None);
        assert!(!cfg.default.keep_source_summaries);
        assert!(cfg.default.skip_paths.is_empty());
    }

    #[test]
    fn load_full_config_overrides_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(
            &path,
            r#"
[default]
vault_path = "/v"
alluvium_subdir = "MyNotes"
recipe = "verbose"
model = "claude-sonnet-4-6"
keep_source_summaries = true
skip_paths = ["/private", "/tmp"]
"#,
        );
        let cfg = load(&path).unwrap();
        assert_eq!(cfg.default.alluvium_subdir, "MyNotes");
        assert_eq!(cfg.default.recipe, "verbose");
        assert_eq!(cfg.default.model.as_deref(), Some("claude-sonnet-4-6"));
        assert!(cfg.default.keep_source_summaries);
        assert_eq!(cfg.default.skip_paths.len(), 2);
    }

    #[test]
    fn load_reserved_profiles_table_is_ignored_but_doesnt_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(
            &path,
            r#"
[default]
vault_path = "/v"

[profiles.work]
vault_path = "/v-work"
"#,
        );
        let cfg = load(&path).unwrap();
        // v0.1 doesn't honor profiles, but should still parse the file.
        assert!(cfg._reserved_profiles.contains_key("work"));
    }

    #[test]
    fn load_missing_file_returns_useful_error() {
        let err = load(Path::new("/nonexistent/config.toml")).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("reading config file"));
    }

    #[test]
    fn load_malformed_toml_returns_useful_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write_config(&path, "this = = not = toml");
        let err = load(&path).unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("parsing config file"));
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let cfg = ConfigFile {
            default: ProfileConfig {
                vault_path: PathBuf::from("/v"),
                alluvium_subdir: "MyNotes".into(),
                recipe: "minimalist".into(),
                model: Some("claude-haiku-4-5".into()),
                backend: Some("claude-cli".into()),
                keep_source_summaries: true,
                skip_paths: vec![PathBuf::from("/skip")],
            },
            _reserved_profiles: Default::default(),
        };
        save(&path, &cfg).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded.default.vault_path, cfg.default.vault_path);
        assert_eq!(loaded.default.recipe, cfg.default.recipe);
        assert!(loaded.default.keep_source_summaries);
    }

    #[test]
    fn default_config_path_is_under_user_config_dir() {
        // Don't compare exact paths (depends on platform); just verify the
        // function returns something rooted at $HOME.
        let path = default_config_path().unwrap();
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"));
        if let Ok(home) = home {
            assert!(
                path.to_string_lossy().contains(&home)
                    || path.to_string_lossy().contains("alluvium"),
                "expected path under home or containing 'alluvium', got: {path:?}"
            );
        }
        assert!(path.to_string_lossy().ends_with("config.toml"));
    }
}
