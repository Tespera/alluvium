//! `alluvium dry-run` — distill the most recent session without writing.
//!
//! Runs the full archive pipeline up to the LLM call + parse, then prints
//! the resulting facts as JSON to stdout instead of writing to the vault.
//! Useful for testing prompt changes before committing.

use anyhow::{Context, Result};

use crate::cli::paths;
use crate::config;
use crate::distiller::{self, DistillerInput};
use crate::transcript::{self, ConversationData};

pub async fn run() -> Result<()> {
    let cfg = load_config()?;
    let api_key = config::secrets::get_api_key()?;

    // Find the most recent transcript file under ~/.claude/projects/.
    let transcript_path =
        find_most_recent_transcript().context("locating most recent Claude Code transcript")?;
    eprintln!(
        "dry-run: using most recent transcript {}",
        transcript_path.display()
    );

    let events = transcript::jsonl::read(&transcript_path)?;
    if events.is_empty() {
        anyhow::bail!("transcript has no events; nothing to distill");
    }

    let metadata = transcript::metadata::extract(&events)?;
    let messages = transcript::reconstruct::run(&events);
    let conversation = ConversationData {
        session_id: metadata.session_id.clone(),
        messages,
        metadata,
    };

    let prompts_dir = paths::prompts_dir()?;
    let template =
        distiller::prompt::load(&cfg.default.recipe, &prompts_dir).with_context(|| {
            format!(
                "loading recipe '{}' from {}",
                cfg.default.recipe,
                prompts_dir.display()
            )
        })?;
    let request = distiller::prompt::render(
        &template,
        &DistillerInput {
            conversation,
            recipe_name: cfg.default.recipe.clone(),
        },
    )?;

    let client = distiller::client::AnthropicClient::new(api_key);
    let response = client.messages(request).await?;
    let output = distiller::parser::parse(&response)?;

    // Print result to stdout as pretty JSON.
    let json = serde_json::to_string_pretty(&output)?;
    println!("{json}");
    Ok(())
}

fn load_config() -> Result<config::ConfigFile> {
    let path = paths::config_file()?;
    config::load(&path)
        .with_context(|| format!("no config at {}; run `alluvium init` first", path.display()))
}

fn find_most_recent_transcript() -> Result<std::path::PathBuf> {
    let projects = home_dir()
        .context("no home directory")?
        .join(".claude")
        .join("projects");
    if !projects.exists() {
        anyhow::bail!(
            "Claude Code transcripts directory does not exist: {}",
            projects.display()
        );
    }
    let mut newest: Option<(std::time::SystemTime, std::path::PathBuf)> = None;
    for project_entry in std::fs::read_dir(&projects)?.flatten() {
        let project_dir = project_entry.path();
        if !project_dir.is_dir() {
            continue;
        }
        for jsonl in std::fs::read_dir(&project_dir)?.flatten() {
            let p = jsonl.path();
            if p.extension().is_some_and(|e| e == "jsonl") {
                if let Ok(meta) = std::fs::metadata(&p) {
                    if let Ok(modified) = meta.modified() {
                        if newest.as_ref().map_or(true, |(t, _)| modified > *t) {
                            newest = Some((modified, p));
                        }
                    }
                }
            }
        }
    }
    newest
        .map(|(_, p)| p)
        .ok_or_else(|| anyhow::anyhow!("no .jsonl transcripts found in {}", projects.display()))
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}
