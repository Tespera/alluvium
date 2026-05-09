//! `alluvium init` — interactive setup wizard.
//!
//! Walks the user through:
//!   1. Vault path
//!   2. Subdirectory inside vault for Alluvium content
//!   3. Recipe selection (default = dev-journal)
//!   4. Anthropic API key (skipped if `$ANTHROPIC_API_KEY` already set)
//!   5. Persists config + secrets
//!   6. Installs bundled prompts to `<config>/prompts/`
//!   7. Prints `claude plugin install <path>` instruction (per ADR-004)

use anyhow::{Context, Result};
use dialoguer::{theme::ColorfulTheme, Input, Password, Select};
use std::path::PathBuf;

use crate::cli::paths;
use crate::config::{self, ConfigFile, ProfileConfig};
use crate::hook::plugin_manifest;

pub async fn run() -> Result<()> {
    println!("\n  Alluvium · setup wizard\n");
    println!("  This will configure Alluvium to archive your Claude Code");
    println!("  sessions into an Obsidian vault. Press Ctrl+C to cancel.\n");

    let theme = ColorfulTheme::default();

    // 1. Vault path.
    let default_vault = home_dir()
        .map(|h| {
            h.join("Documents")
                .join("MyVault")
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| "/path/to/vault".into());
    let vault: String = Input::with_theme(&theme)
        .with_prompt("Path to your Obsidian vault")
        .default(default_vault)
        .interact_text()?;
    let vault_path = PathBuf::from(vault.trim());
    if !vault_path.exists() {
        println!(
            "  ⚠  vault path does not exist yet: {}",
            vault_path.display()
        );
        println!("     (Alluvium will create the Alluvium subdirectory on first archive.)");
    }

    // 2. Subdirectory.
    let subdir: String = Input::with_theme(&theme)
        .with_prompt("Subdirectory inside the vault for Alluvium content")
        .default("Alluvium".into())
        .interact_text()?;

    // 3. Recipe.
    let recipes = ["dev-journal", "minimalist", "verbose"];
    let recipe_idx = Select::with_theme(&theme)
        .with_prompt("Distillation recipe (controls note style)")
        .items(&recipes)
        .default(0)
        .interact()?;
    let recipe = recipes[recipe_idx].to_string();

    // 4. API key.
    let api_key_already_in_env = std::env::var("ANTHROPIC_API_KEY").is_ok_and(|v| !v.is_empty());
    if !api_key_already_in_env {
        let key: String = Password::with_theme(&theme)
            .with_prompt("Anthropic API key (sk-ant-...) — stored in your OS keychain")
            .interact()?;
        if key.trim().is_empty() {
            anyhow::bail!("no API key provided; rerun `alluvium init` to retry");
        }
        config::secrets::set_api_key(key.trim()).context("storing API key in OS keychain")?;
        println!("  ✓ API key stored in keychain.");
    } else {
        println!("  ✓ Using $ANTHROPIC_API_KEY from environment.");
    }

    // 5. Persist config.
    let cfg = ConfigFile {
        default: ProfileConfig {
            vault_path: vault_path.clone(),
            alluvium_subdir: subdir.trim().to_string(),
            recipe: recipe.clone(),
            model: "claude-haiku-4-5".into(),
            keep_source_summaries: false,
            skip_paths: detect_default_skip_paths(),
        },
        _reserved_profiles: Default::default(),
    };
    let config_path = paths::config_file()?;
    config::save(&config_path, &cfg)
        .with_context(|| format!("writing config to {}", config_path.display()))?;
    println!("  ✓ Config written to {}", config_path.display());

    // 6. Install bundled prompts.
    let prompts_dir = paths::prompts_dir()?;
    paths::install_bundled_prompts(&prompts_dir).context("installing bundled prompts")?;
    println!(
        "  ✓ Prompt templates installed to {}",
        prompts_dir.display()
    );
    println!("    (Edit them anytime; Alluvium reads on each archive.)");

    // 7. Plugin install instruction.
    let plugin_root =
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/path/to/alluvium"));
    let manifest_path = plugin_root.join(".claude-plugin").join("plugin.json");
    if manifest_path.exists() {
        println!("\n{}", plugin_manifest::install_instruction(&plugin_root));
    } else {
        println!(
            "\n  ⚠  Could not locate .claude-plugin/plugin.json from cwd ({}).",
            plugin_root.display()
        );
        println!("     If you have the Alluvium repo checked out, cd there and run:");
        println!("        claude plugin install $(pwd)");
    }

    println!("\n  Setup complete.\n");
    println!("  Next steps:");
    println!("    1. Run the plugin install command above (Claude Code).");
    println!("    2. Use Claude Code normally. After a session ends, your");
    println!(
        "       distilled notes will appear in {}.",
        vault_path.join(&cfg.default.alluvium_subdir).display()
    );
    println!("    3. Run `alluvium status` to see recent archives.");
    println!("    4. Run `alluvium dry-run` anytime to preview the next");
    println!("       distillation without writing to your vault.\n");

    Ok(())
}

/// Try to auto-detect a sensible default for skip_paths: if `cwd` is itself
/// the Alluvium dev tree (i.e. a directory containing a Cargo.toml whose
/// package name is "alluvium"), include it so the user doesn't archive
/// their own work on Alluvium itself.
fn detect_default_skip_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        if is_alluvium_dev_tree(&cwd) {
            out.push(cwd);
        }
    }
    out
}

fn is_alluvium_dev_tree(dir: &std::path::Path) -> bool {
    let cargo = dir.join("Cargo.toml");
    if !cargo.exists() {
        return false;
    }
    let content = match std::fs::read_to_string(&cargo) {
        Ok(c) => c,
        Err(_) => return false,
    };
    content.contains("name = \"alluvium\"")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}
