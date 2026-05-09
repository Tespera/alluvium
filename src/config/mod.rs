//! Configuration loading & writing.
//!
//! - Reads `~/.config/alluvium/config.toml` (or platform equivalent).
//! - Schema is `[default]` with optional future-reserved `[profiles.<name>]`.
//! - Use [`secrets`] for the Anthropic API key (Keychain).

pub mod secrets;

use serde::{Deserialize, Serialize};

/// Top-level config file structure.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigFile {
    pub default: ProfileConfig,
    // v0.2: profiles will live here. Loaded but not honored in v0.1.
    #[serde(default, rename = "profiles")]
    pub _reserved_profiles: std::collections::BTreeMap<String, ProfileConfig>,
}

/// Per-profile settings (v0.1 only uses `default`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProfileConfig {
    pub vault_path: std::path::PathBuf,
    #[serde(default = "default_subdir")]
    pub alluvium_subdir: String,
    #[serde(default = "default_recipe")]
    pub recipe: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub keep_source_summaries: bool,
    #[serde(default)]
    pub skip_paths: Vec<std::path::PathBuf>,
}

fn default_subdir() -> String {
    "Alluvium".into()
}
fn default_recipe() -> String {
    "dev-journal".into()
}
fn default_model() -> String {
    "claude-haiku-4-5".into()
}
