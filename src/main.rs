//! LLM Debugger CLI - A debugger interface for LLM coding agents
//!
//! This CLI tool uses the Debug Adapter Protocol (DAP) to provide debugging
//! capabilities through a simple command-line interface optimized for LLM agents.

use clap::Parser;
use debugger::{cli, commands, daemon};
use commands::Commands;

#[derive(Parser)]
#[command(name = "debugger", about = "LLM-friendly debugger CLI")]
#[command(version, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Daemon => daemon::run().await,
        command => cli::dispatch(command).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
