//! Logging and tracing configuration
//!
//! Provides structured logging for both CLI and daemon modes.
//! The daemon logs to a file since it runs in the background.

use std::path::PathBuf;
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

use super::paths;

/// Initialize tracing for the CLI (stdout logging)
///
/// Logs are controlled by the `RUST_LOG` environment variable.
/// Default level is INFO for this crate, WARN for dependencies.
pub fn init_cli() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("debugger=info,warn")
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(false)
                .with_file(false)
                .with_line_number(false)
                .compact(),
        )
        .init();
}

/// Initialize tracing for the daemon (file + stderr logging)
///
/// The daemon logs to both:
/// 1. A log file at `~/.local/share/debugger-cli/logs/daemon.log`
/// 2. stderr (inherited from spawning process for early errors)
///
/// Log level controlled by `RUST_LOG`, default is TRACE for daemon to capture DAP messages.
pub fn init_daemon() -> Option<PathBuf> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Default to trace for daemon - we want to see DAP messages
        EnvFilter::new("debugger=trace,info")
    });

    // Try to set up file logging
    let log_path = if let Some(log_dir) = paths::log_dir() {
        // Ensure log directory exists
        if std::fs::create_dir_all(&log_dir).is_ok() {
            let log_file = log_dir.join("daemon.log");

            // Create or append to log file
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_file)
            {
                Ok(file) => {
                    // File logging with full details
                    let file_layer = fmt::layer()
                        .with_writer(file)
                        .with_ansi(false)
                        .with_target(true)
                        .with_thread_ids(true)
                        .with_file(true)
                        .with_line_number(true)
                        .with_span_events(FmtSpan::ENTER | FmtSpan::EXIT);

                    // Also log to stderr for early startup issues
                    let stderr_layer = fmt::layer()
                        .with_writer(std::io::stderr)
                        .with_target(true)
                        .with_thread_ids(false)
                        .with_file(false)
                        .compact();

                    tracing_subscriber::registry()
                        .with(filter)
                        .with(file_layer)
                        .with(stderr_layer)
                        .init();

                    return Some(log_file);
                }
                Err(e) => {
                    eprintln!("Warning: Could not open log file: {}", e);
                }
            }
        }
        None
    } else {
        None
    };

    // Fallback: stderr only
    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true),
        )
        .init();

    log_path
}

/// Get the path to the daemon log file
pub fn daemon_log_path() -> Option<PathBuf> {
    paths::log_dir().map(|d| d.join("daemon.log"))
}

/// Truncate the daemon log file (useful before debugging sessions)
pub fn truncate_daemon_log() -> std::io::Result<()> {
    if let Some(path) = daemon_log_path() {
        if path.exists() {
            std::fs::write(&path, "")?;
        }
    }
    Ok(())
}
