//! LLM Debugger CLI - A debugger interface for LLM coding agents
//!
//! This CLI tool uses the Debug Adapter Protocol (DAP) to provide debugging
//! capabilities through a simple command-line interface optimized for LLM agents.

use clap::Parser;
use debugger::commands::Commands;
use debugger::common::logging;
use debugger::{cli, daemon};

#[derive(Parser)]
#[command(name = "debugger", about = "LLM-friendly debugger CLI")]
#[command(version, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Initialize logging differently for daemon vs CLI mode
    let is_daemon = matches!(cli.command, Commands::Daemon);
    if is_daemon {
        if let Some(log_path) = logging::init_daemon() {
            eprintln!("Daemon logging to: {}", log_path.display());
        }
    } else {
        logging::init_cli();
    }

    let result = match cli.command {
        Commands::Daemon => daemon::run().await,
        command => cli::dispatch(command).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
