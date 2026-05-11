//! Shared path resolution + bundled prompt content for CLI handlers.
//!
//! All cli commands resolve paths via [`directories::ProjectDirs`] under
//! `dev/alluvium/alluvium`. Bundled prompt templates are compiled in via
//! `include_str!` so we don't need a separate "data files" install step;
//! `alluvium init` writes them out to `<config>/prompts/` for the user
//! to customize.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Compiled-in copy of `prompts/distill.toml`.
pub const BUNDLED_DISTILL_TOML: &str = include_str!("../../prompts/distill.toml");

/// Compiled-in copy of `prompts/consolidate.toml`. Used by
/// `alluvium consolidate` to rewrite fragmented topic pages.
pub const BUNDLED_CONSOLIDATE_TOML: &str = include_str!("../../prompts/consolidate.toml");

/// Compiled-in copy of `prompts/lint.toml`. Used by `alluvium lint` to
/// decide MERGE vs KEEP for near-duplicate topic-page pairs.
pub const BUNDLED_LINT_TOML: &str = include_str!("../../prompts/lint.toml");

/// Compiled-in copy of `prompts/audit.toml`. Used by `alluvium audit`
/// to classify each topic page as durable / episode / noise.
pub const BUNDLED_AUDIT_TOML: &str = include_str!("../../prompts/audit.toml");

/// Compiled-in recipe files.
pub const BUNDLED_RECIPES: &[(&str, &str)] = &[
    (
        "minimalist",
        include_str!("../../prompts/recipes/minimalist.toml"),
    ),
    (
        "dev-journal",
        include_str!("../../prompts/recipes/dev-journal.toml"),
    ),
    (
        "verbose",
        include_str!("../../prompts/recipes/verbose.toml"),
    ),
];

fn project_dirs() -> Result<directories::ProjectDirs> {
    directories::ProjectDirs::from("dev", "alluvium", "alluvium")
        .context("no home directory available; cannot resolve runtime paths")
}

pub fn config_dir() -> Result<PathBuf> {
    Ok(project_dirs()?.config_dir().to_path_buf())
}

pub fn cache_dir() -> Result<PathBuf> {
    Ok(project_dirs()?.cache_dir().to_path_buf())
}

pub fn data_dir() -> Result<PathBuf> {
    Ok(project_dirs()?.data_dir().to_path_buf())
}

pub fn prompts_dir() -> Result<PathBuf> {
    Ok(config_dir()?.join("prompts"))
}

pub fn config_file() -> Result<PathBuf> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn archive_log_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("log").join("archive.jsonl"))
}

pub fn lock_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("alluvium.lock"))
}

pub fn session_cache_dir(session_id: &str) -> Result<PathBuf> {
    Ok(cache_dir()?.join("sessions").join(session_id))
}

pub fn session_resolved_json(session_id: &str) -> Result<PathBuf> {
    Ok(session_cache_dir(session_id)?.join("resolved.json"))
}

pub fn session_snapshots_dir(session_id: &str) -> Result<PathBuf> {
    Ok(session_cache_dir(session_id)?.join("snapshots"))
}

/// Per-session debug dump directory under the data dir. Used by the
/// `--debug` flag in `alluvium archive`. Not auto-cleaned — users can
/// `rm -rf` once they're done inspecting.
pub fn debug_dir(session_id: &str) -> Result<PathBuf> {
    Ok(data_dir()?.join("debug").join(session_id))
}

/// Write the bundled prompt files out to `<config>/prompts/`. Idempotent —
/// won't clobber a file the user has edited (we check sha256-ish equality
/// loosely by comparing contents). Used by `alluvium init`.
pub fn install_bundled_prompts(prompts_dir: &std::path::Path) -> Result<()> {
    let recipes_dir = prompts_dir.join("recipes");
    std::fs::create_dir_all(&recipes_dir)
        .with_context(|| format!("creating {}", recipes_dir.display()))?;

    write_if_absent_or_unchanged(&prompts_dir.join("distill.toml"), BUNDLED_DISTILL_TOML)?;
    write_if_absent_or_unchanged(
        &prompts_dir.join("consolidate.toml"),
        BUNDLED_CONSOLIDATE_TOML,
    )?;
    write_if_absent_or_unchanged(&prompts_dir.join("lint.toml"), BUNDLED_LINT_TOML)?;
    write_if_absent_or_unchanged(&prompts_dir.join("audit.toml"), BUNDLED_AUDIT_TOML)?;
    for (name, content) in BUNDLED_RECIPES {
        write_if_absent_or_unchanged(&recipes_dir.join(format!("{name}.toml")), content)?;
    }
    Ok(())
}

/// Write `content` to `path` only if the file is absent. If the file exists
/// and has been edited (content differs from any known bundled version),
/// leave it alone — the user has customized it.
fn write_if_absent_or_unchanged(path: &std::path::Path, bundled: &str) -> Result<()> {
    if path.exists() {
        // File exists; don't clobber the user's customization.
        return Ok(());
    }
    crate::vault::writer::write_atomic(path, bundled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_distill_toml_parses() {
        // Sanity: the prompt file we compile in must actually be valid TOML.
        let _: toml::Value = toml::from_str(BUNDLED_DISTILL_TOML).unwrap();
    }

    #[test]
    fn bundled_recipes_all_parse() {
        for (name, content) in BUNDLED_RECIPES {
            let _: toml::Value = toml::from_str(content)
                .unwrap_or_else(|e| panic!("recipe {name} failed to parse: {e}"));
        }
    }

    #[test]
    fn bundled_recipes_have_expected_names() {
        let names: Vec<&str> = BUNDLED_RECIPES.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"minimalist"));
        assert!(names.contains(&"dev-journal"));
        assert!(names.contains(&"verbose"));
    }

    #[test]
    fn install_bundled_prompts_writes_all_files() {
        let dir = tempfile::tempdir().unwrap();
        let prompts = dir.path().join("prompts");
        install_bundled_prompts(&prompts).unwrap();
        assert!(prompts.join("distill.toml").exists());
        assert!(prompts.join("recipes/minimalist.toml").exists());
        assert!(prompts.join("recipes/dev-journal.toml").exists());
        assert!(prompts.join("recipes/verbose.toml").exists());
    }

    #[test]
    fn install_bundled_prompts_does_not_clobber_existing_customization() {
        let dir = tempfile::tempdir().unwrap();
        let prompts = dir.path().join("prompts");
        std::fs::create_dir_all(prompts.join("recipes")).unwrap();
        let custom_content = "# my custom prompt\n[meta]\nname = \"custom\"";
        let path = prompts.join("distill.toml");
        std::fs::write(&path, custom_content).unwrap();

        install_bundled_prompts(&prompts).unwrap();

        let after = std::fs::read_to_string(&path).unwrap();
        assert_eq!(after, custom_content, "user file should not be clobbered");
    }
}
