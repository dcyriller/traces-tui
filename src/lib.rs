//! Triage LangSmith agent traces from the terminal and hand the threads to fix to Claude Code.
pub mod cli;
pub mod commands;
pub mod config;
pub mod ctx;
pub mod fix;
pub mod langsmith;
pub mod marks;
pub mod model;
pub mod prompt;
mod turn;

use anyhow::{Result, bail};
use cli::{Cli, Command};
use ctx::{Ctx, Env};
use std::io::{stdin, stdout};

/// Runs the command line `cli` asks for.
pub async fn run(cli: Cli) -> Result<()> {
    let ctx = Ctx::build(&cli, Env::capture()?)?;
    match &cli.command {
        Some(Command::Marks(args)) => commands::marks(&ctx, args, &mut stdout()),
        Some(Command::Fix(args)) => commands::fix(&ctx, args, &mut stdin().lock(), &mut stdout()),
        Some(Command::Url { thread_id }) => commands::url(&ctx, thread_id, &mut stdout()).await,
        None => bail!("the TUI is not built yet"),
    }
}
