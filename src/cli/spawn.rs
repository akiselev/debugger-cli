//! Daemon spawning logic
//!
//! Automatically spawns the daemon process when needed, using the same binary
//! with the hidden `daemon` subcommand.

use std::time::Duration;

use crate::common::{paths, Error, Result};
use crate::ipc::{transport, DaemonClient};

/// Timeout for daemon to start up
const SPAWN_TIMEOUT_SECS: u64 = 5;

/// Ensure the daemon is running, spawning it if necessary
pub async fn ensure_daemon_running() -> Result<()> {
    // Try to connect first
    match DaemonClient::connect().await {
        Ok(_) => return Ok(()), // Already running
        Err(Error::DaemonNotRunning) => {
            // Need to spawn
            spawn_daemon().await?;
        }
        Err(e) => return Err(e),
    }

    Ok(())
}

/// Spawn the daemon process
async fn spawn_daemon() -> Result<()> {
    tracing::debug!("Spawning daemon process");

    // Get path to current executable
    let exe_path = std::env::current_exe().map_err(|e| {
        Error::Internal(format!("Failed to get current executable path: {}", e))
    })?;

    // Ensure socket directory exists
    paths::ensure_socket_dir()?;

    // Remove stale socket if it exists
    paths::remove_socket()?;

    // Spawn detached process with output redirected to /dev/null
    // The daemon logs to its own log file, so we don't need terminal output
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        use std::fs::File;
        
        // Open /dev/null for stdout/stderr
        let dev_null = File::open("/dev/null")
            .map_err(|e| Error::Internal(format!("Failed to open /dev/null: {}", e)))?;
        let dev_null_out = File::create("/dev/null")
            .map_err(|e| Error::Internal(format!("Failed to open /dev/null for write: {}", e)))?;
        
        std::process::Command::new(&exe_path)
            .arg("daemon")
            .stdin(std::process::Stdio::from(dev_null))
            .stdout(std::process::Stdio::from(dev_null_out.try_clone().unwrap()))
            .stderr(std::process::Stdio::from(dev_null_out))
            .process_group(0) // New process group (detach from terminal)
            .spawn()
            .map_err(|e| Error::Internal(format!("Failed to spawn daemon: {}", e)))?;
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        std::process::Command::new(&exe_path)
            .arg("daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP)
            .spawn()
            .map_err(|e| Error::Internal(format!("Failed to spawn daemon: {}", e)))?;
    }

    // Wait for daemon to start accepting connections
    let deadline = std::time::Instant::now() + Duration::from_secs(SPAWN_TIMEOUT_SECS);

    loop {
        if std::time::Instant::now() >= deadline {
            return Err(Error::DaemonSpawnTimeout(SPAWN_TIMEOUT_SECS));
        }

        // Try to connect
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Check if socket exists (Unix only)
        #[cfg(unix)]
        if !paths::socket_path().exists() {
            continue;
        }

        // Try to connect
        match transport::connect().await {
            Ok(_) => {
                tracing::debug!("Daemon started successfully");
                return Ok(());
            }
            Err(_) => continue,
        }
    }
}
