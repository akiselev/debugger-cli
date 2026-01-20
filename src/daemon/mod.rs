//! Daemon mode - background process managing DAP adapter
//!
//! The daemon is spawned automatically by CLI commands and maintains
//! persistent debug sessions across CLI invocations.

mod handler;
mod server;
mod session;

use crate::common::Result;

/// Run in daemon mode
///
/// This is the entry point when the binary is invoked with the hidden `daemon` command.
/// The daemon:
/// 1. Creates an IPC socket/pipe for CLI connections
/// 2. Accepts CLI commands and translates them to DAP requests
/// 3. Buffers events when no client is connected
/// 4. Manages the debug session lifecycle
pub async fn run() -> Result<()> {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        pid = std::process::id(),
        "Starting debugger daemon"
    );

    let mut daemon = server::Daemon::new().await?;
    daemon.run().await
}
