//! Triage LangSmith agent traces from the terminal and hand the threads to fix to Claude Code.
pub mod cli;

use anyhow::Result;
use cli::Cli;

/// Runs the command line `cli` asks for.
pub fn run(cli: &Cli) -> Result<()> {
    let _ = cli.command;
    Ok(())
}
