//! Alluvium CLI entry point.
//!
//! Subcommand contracts are documented in `docs/HOOKS.md` (for hook handlers)
//! and `docs/ARCHITECTURE.md` (for user-facing commands).

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "alluvium",
    version,
    about = "Auto-archive Claude Code sessions to your Obsidian vault.",
    long_about = "Alluvium watches Claude Code sessions end, distills the transcript into knowledge, and merges it into a Karpathy-style LLM wiki inside your Obsidian vault. See docs/ for design details."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Interactive setup wizard: configure vault, API key, recipe; install Claude Code plugin.
    Init,

    /// Distill a Claude Code session and write to vault. Called by the Stop hook.
    Archive {
        /// Claude Code session id (forwarded by the Stop hook).
        #[arg(long)]
        session: String,
    },

    /// Re-distill an old session or batch (e.g., after editing prompts).
    Replay {
        /// Specific session id to replay.
        session: Option<String>,

        /// Replay all sessions newer than this (e.g., "7d", "24h", "2026-05-01").
        #[arg(long)]
        since: Option<String>,

        /// Replay every session ever recorded.
        #[arg(long)]
        all: bool,
    },

    /// Show recent archive summaries and any errors.
    Status,

    /// Distill the most recent session without writing to vault (preview only).
    DryRun,

    /// Have the LLM rewrite fragmented topic pages into tighter prose.
    Consolidate,

    /// SessionStart hook handler.
    SessionStart,

    /// PreCompact hook handler.
    PreCompact,

    /// SessionEnd hook handler.
    SessionEnd,

    /// Uninstall the Claude Code plugin (config preserved).
    Uninstall,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Init => alluvium::cli::init::run().await,
        Command::Archive { session } => alluvium::cli::archive::run(&session).await,
        Command::Replay {
            session,
            since,
            all,
        } => alluvium::cli::replay::run(session.as_deref(), since.as_deref(), all).await,
        Command::Status => alluvium::cli::status::run().await,
        Command::DryRun => alluvium::cli::dry_run::run().await,
        Command::Consolidate => alluvium::cli::consolidate::run().await,
        Command::SessionStart => alluvium::cli::session_start::run().await,
        Command::PreCompact => alluvium::cli::pre_compact::run().await,
        Command::SessionEnd => alluvium::cli::session_end::run().await,
        Command::Uninstall => alluvium::cli::uninstall::run().await,
    }
}
