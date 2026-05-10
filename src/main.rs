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
    /// Dump distiller input / raw LLM output / parsed output JSON to
    /// `<data_dir>/debug/<session>/` for post-mortem inspection. Useful when
    /// archive fails or produces unexpected pages.
    #[arg(long, global = true)]
    debug: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Interactive setup wizard: configure vault, API key, recipe; install Claude Code plugin.
    Init,

    /// Distill a Claude Code session and write to vault.
    ///
    /// Two modes:
    /// - **Hook mode** (default): no `--session` arg; the binary reads the
    ///   Claude Code hook payload from stdin (JSON with session_id,
    ///   transcript_path, cwd, ...).
    /// - **Manual mode**: pass `--session <id>` to re-archive a specific session.
    Archive {
        /// Claude Code session id (only for manual / replay use; hook mode reads stdin).
        #[arg(long)]
        session: Option<String>,

        /// Hook entry-point: read stdin payload, spawn a detached worker,
        /// and return immediately so the Stop hook does not block Claude Code.
        /// The plugin manifest sets this; users do not pass it manually.
        #[arg(long)]
        detached: bool,
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
    ///
    /// v0.1 is single-page: pass the slug of the page to consolidate.
    /// `--all` and scheduled runs are v0.2 work.
    Consolidate {
        /// Topic page slug (the filename stem under `wiki/concepts/` or
        /// `wiki/entities/`). For example, for `wiki/concepts/atomic-write.md`
        /// pass `atomic-write`.
        slug: String,
    },

    /// Health-check the wiki: find near-duplicate topic pages and ask
    /// the LLM whether they should be merged.
    ///
    /// Default is dry-run (prints suggestions only). Pass `--apply` to
    /// actually rewrite winners and delete losers.
    Lint {
        /// Actually apply the merges. Without this flag, lint is a
        /// read-only preview.
        #[arg(long)]
        apply: bool,
    },

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
    alluvium::set_debug_flag(cli.debug);

    match cli.command {
        Command::Init => alluvium::cli::init::run().await,
        Command::Archive { session, detached } => {
            alluvium::cli::archive::run(session.as_deref(), detached).await
        }
        Command::Replay {
            session,
            since,
            all,
        } => alluvium::cli::replay::run(session.as_deref(), since.as_deref(), all).await,
        Command::Status => alluvium::cli::status::run().await,
        Command::DryRun => alluvium::cli::dry_run::run().await,
        Command::Consolidate { slug } => alluvium::cli::consolidate::run(&slug).await,
        Command::Lint { apply } => alluvium::cli::lint::run(apply).await,
        Command::SessionStart => alluvium::cli::session_start::run().await,
        Command::PreCompact => alluvium::cli::pre_compact::run().await,
        Command::SessionEnd => alluvium::cli::session_end::run().await,
        Command::Uninstall => alluvium::cli::uninstall::run().await,
    }
}
