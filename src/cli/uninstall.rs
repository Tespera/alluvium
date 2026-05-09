//! `alluvium uninstall` — print uninstall instructions.
//!
//! v0.1: the install model is "user runs `claude plugin install <path>`",
//! and we don't have an automated registry of where that registered the
//! plugin. So uninstall mirrors that: print the command the user should run
//! (`claude plugin uninstall alluvium`) plus optional cleanup hints.
//!
//! Config and archive log are NOT removed automatically — those represent
//! the user's data + cost-tracking history. We tell them where it lives in
//! case they want to delete manually.

use anyhow::Result;

use crate::cli::paths;

pub async fn run() -> Result<()> {
    let config_path = paths::config_file()?;
    let cache = paths::cache_dir()?;
    let data = paths::data_dir()?;

    println!(
        "Uninstalling Alluvium\n\
         =====================\n\n\
         1. Unregister the Claude Code plugin:\n\n      \
              claude plugin uninstall alluvium\n\n\
         2. (Optional) Remove configuration and runtime state:\n\n      \
              rm {config}\n      \
              rm -rf {cache}\n      \
              rm -rf {data}\n\n\
         3. (Optional) Delete the API key from your OS keychain:\n\n      \
              alluvium config delete-api-key   # not yet implemented in v0.1\n\n   \
              On macOS you can also: open Keychain Access, search for\n   \
              \"alluvium\" / \"anthropic-api-key\", delete the entry.\n\n\
         4. (Optional) Remove the binary itself:\n\n      \
              brew uninstall alluvium   # if installed via Homebrew\n\n\
         Your Obsidian vault is untouched — Alluvium-written pages stay\n\
         where they are. To remove them too, delete the\n\
         <vault>/Alluvium/ subdirectory.\n",
        config = config_path.display(),
        cache = cache.display(),
        data = data.display(),
    );
    Ok(())
}
