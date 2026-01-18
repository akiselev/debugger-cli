//! Cross-platform IPC transport layer
//!
//! Abstracts Unix domain sockets (Unix/macOS) and named pipes (Windows)
//! using the interprocess crate.

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::common::paths;

/// Maximum message size (10 MB)
const MAX_MESSAGE_SIZE: u32 = 10 * 1024 * 1024;

// Platform-specific imports and type aliases
#[cfg(unix)]
pub mod platform {
    pub use interprocess::local_socket::tokio::{
        prelude::*,
        Listener, Stream,
    };
    pub use interprocess::local_socket::{
        GenericFilePath, ListenerOptions,
    };
}

#[cfg(windows)]
pub mod platform {
    pub use interprocess::local_socket::tokio::{
        prelude::*,
        Listener, Stream,
    };
    pub use interprocess::local_socket::{
        GenericNamespaced, ListenerOptions,
    };
}

use platform::*;

/// Re-export Stream for use in other modules
pub use platform::Stream;

/// Create a listener for incoming IPC connections
pub async fn create_listener() -> io::Result<Listener> {
    // Ensure socket directory exists (Unix) and clean up stale socket
    paths::ensure_socket_dir()?;
    paths::remove_socket()?;

    let name = paths::socket_name();

    #[cfg(unix)]
    let listener = {
        let name = name.to_fs_name::<GenericFilePath>()?;
        ListenerOptions::new()
            .name(name)
            .create_tokio()?
    };

    #[cfg(windows)]
    let listener = {
        let name = name.to_ns_name::<GenericNamespaced>()?;
        ListenerOptions::new()
            .name(name)
            .create_tokio()?
    };

    // Set socket permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = paths::socket_path();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(listener)
}

/// Connect to the daemon's IPC socket
pub async fn connect() -> io::Result<Stream> {
    let name = paths::socket_name();

    #[cfg(unix)]
    let stream = {
        let name = name.to_fs_name::<GenericFilePath>()?;
        Stream::connect(name).await?
    };

    #[cfg(windows)]
    let stream = {
        let name = name.to_ns_name::<GenericNamespaced>()?;
        Stream::connect(name).await?
    };

    Ok(stream)
}

/// Send a length-prefixed message
pub async fn send_message<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    data: &[u8],
) -> io::Result<()> {
    if data.len() > MAX_MESSAGE_SIZE as usize {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Message too large",
        ));
    }

    let len = data.len() as u32;
    writer.write_all(&len.to_le_bytes()).await?;
    writer.write_all(data).await?;
    writer.flush().await?;
    Ok(())
}

/// Receive a length-prefixed message
pub async fn recv_message<R: AsyncReadExt + Unpin>(reader: &mut R) -> io::Result<Vec<u8>> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf);

    if len > MAX_MESSAGE_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Message too large: {} bytes", len),
        ));
    }

    let mut data = vec![0u8; len as usize];
    reader.read_exact(&mut data).await?;
    Ok(data)
}

/// Check if the daemon socket exists
pub fn socket_exists() -> bool {
    #[cfg(unix)]
    {
        paths::socket_path().exists()
    }

    #[cfg(windows)]
    {
        // On Windows, we can't easily check if a named pipe exists
        // We'll rely on connection attempts instead
        true
    }
}
