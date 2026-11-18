//! The `traces` binary: parses the command line and runs it.
use clap::Parser;
use std::process::ExitCode;
use traces_tui::cli::Cli;

#[tokio::main]
async fn main() -> ExitCode {
    match traces_tui::run(&Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("traces: {err:#}");
            ExitCode::FAILURE
        }
    }
}
