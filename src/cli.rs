//! The command line.
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

/// `traces`: triage LangSmith traces and hand marked threads to Claude Code.
#[derive(Debug, Parser)]
#[command(name = "traces", version, about)]
pub struct Cli {
    /// The LangSmith project to read, instead of the configured one.
    #[arg(long, global = true)]
    pub project: Option<String>,
    /// The config file, instead of `$TRACES_CONFIG` or `~/.config/traces/config.toml`.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    /// How far back to list thread activity: `5m`, `12h` or `7d`.
    #[arg(long, global = true)]
    pub since: Option<String>,
    /// What to run; the TUI when omitted.
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The subcommands that skip the TUI.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the marked threads.
    Marks(MarksArgs),
    /// Launch the fix command on the marked threads.
    Fix(FixArgs),
    /// Print the LangSmith link of a thread.
    Url {
        /// The thread id.
        thread_id: String,
    },
}

/// Flags of `traces marks`.
#[derive(Debug, Args)]
pub struct MarksArgs {
    /// Print JSON instead of a table.
    #[arg(long)]
    pub json: bool,
    /// Only marks in this status.
    #[arg(long, value_parser = ["todo", "sent"])]
    pub status: Option<String>,
}

/// Flags of `traces fix`.
#[derive(Debug, Args)]
pub struct FixArgs {
    /// Print the command and the prompt instead of running them.
    #[arg(long)]
    pub dry_run: bool,
    /// Skip the confirmation.
    #[arg(long)]
    pub yes: bool,
    /// Include marks already sent.
    #[arg(long)]
    pub all: bool,
}
