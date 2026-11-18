//! Triage LangSmith agent traces from the terminal and hand the threads to fix to Claude Code.
pub mod cli;
pub mod config;
pub mod ctx;
#[cfg_attr(not(test), expect(dead_code, reason = "the client reads them in the next step"))]
pub mod langsmith;
pub mod model;
#[cfg_attr(not(test), expect(dead_code, reason = "the client reads them in the next step"))]
mod turn;

use anyhow::Result;
use cli::Cli;

/// Runs the command line `cli` asks for.
pub fn run(cli: &Cli) -> Result<()> {
    let _ = cli.command;
    Ok(())
}
