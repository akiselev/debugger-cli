//! Cross-platform socket and configuration paths
//!
//! Unix/macOS: Uses Unix domain sockets at $XDG_RUNTIME_DIR or /tmp
//! Windows: Uses named pipes at \\.\pipe\debugger-cli-<username>

use std::io;
use std::path::PathBuf;

/// Name used for the IPC socket/pipe
const SOCKET_NAME: &str = "debugger-cli";

/// Get the socket/pipe path for IPC communication
///
/// Platform-specific:
/// - Unix: `$XDG_RUNTIME_DIR/debugger-cli/daemon.sock` or `/tmp/debugger-cli-<uid>/daemon.sock`
/// - Windows: Named pipe path (handled by interprocess crate)
#[cfg(unix)]
pub fn socket_path() -> PathBuf {
    // Try XDG_RUNTIME_DIR first (preferred on Linux)
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        return PathBuf::from(runtime_dir)
            .join(SOCKET_NAME)
            .join("daemon.sock");
    }

    // Fallback to /tmp with uid for security
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/{}-{}", SOCKET_NAME, uid)).join("daemon.sock")
}

#[cfg(windows)]
pub fn socket_path() -> PathBuf {
    // On Windows, we return a path that will be converted to a named pipe
    // The interprocess crate handles the \\.\pipe\ prefix
    let username = std::env::var("USERNAME").unwrap_or_else(|_| "default".to_string());
    PathBuf::from(format!("{}-{}", SOCKET_NAME, username))
}

/// Get the socket name for interprocess LocalSocketName
///
/// Returns a string suitable for use with interprocess crate's local socket API
#[cfg(unix)]
pub fn socket_name() -> String {
    socket_path().to_string_lossy().into_owned()
}

#[cfg(windows)]
pub fn socket_name() -> String {
    let username = std::env::var("USERNAME").unwrap_or_else(|_| "default".to_string());
    format!("{}-{}", SOCKET_NAME, username)
}

/// Ensure the socket directory exists with proper permissions
///
/// On Unix, creates the directory with mode 0700 for security
#[cfg(unix)]
pub fn ensure_socket_dir() -> io::Result<PathBuf> {
    let socket = socket_path();
    let dir = socket.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Invalid socket path")
    })?;

    if !dir.exists() {
        std::fs::create_dir_all(dir)?;
        // Set directory permissions to 0700 (owner only)
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }

    Ok(dir.to_path_buf())
}

#[cfg(windows)]
pub fn ensure_socket_dir() -> io::Result<PathBuf> {
    // Named pipes don't need a directory on Windows
    Ok(PathBuf::new())
}

/// Remove the socket file if it exists (for cleanup)
#[cfg(unix)]
pub fn remove_socket() -> io::Result<()> {
    let path = socket_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(windows)]
pub fn remove_socket() -> io::Result<()> {
    // Named pipes are automatically cleaned up on Windows
    Ok(())
}

/// Get the configuration directory path
///
/// Uses the directories crate for platform-appropriate locations:
/// - Linux: `~/.config/debugger-cli/`
/// - macOS: `~/Library/Application Support/debugger-cli/`
/// - Windows: `%APPDATA%\debugger-cli\`
pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", SOCKET_NAME)
        .map(|dirs| dirs.config_dir().to_path_buf())
}

/// Get the path to the configuration file
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.toml"))
}

/// Get the path to the log directory
pub fn log_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", SOCKET_NAME)
        .map(|dirs| dirs.data_dir().join("logs"))
}

/// Ensure the configuration directory exists
pub fn ensure_config_dir() -> io::Result<Option<PathBuf>> {
    if let Some(dir) = config_dir() {
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(Some(dir))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_path_is_valid() {
        let path = socket_path();
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn test_config_dir_is_valid() {
        let dir = config_dir();
        assert!(dir.is_some());
    }
}
