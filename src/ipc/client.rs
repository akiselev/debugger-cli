//! CLI-side IPC client for communicating with the daemon

use tokio::io::{ReadHalf, WriteHalf};

use crate::common::{Error, Result};

use super::protocol::{Command, Request, Response};
use super::transport::{self, Stream};

/// Client for communicating with the debugger daemon
pub struct DaemonClient {
    reader: ReadHalf<Stream>,
    writer: WriteHalf<Stream>,
    next_id: u64,
}

impl DaemonClient {
    /// Connect to the running daemon
    pub async fn connect() -> Result<Self> {
        let stream = transport::connect().await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound
                || e.kind() == std::io::ErrorKind::ConnectionRefused
            {
                Error::DaemonNotRunning
            } else {
                Error::DaemonConnectionFailed(e)
            }
        })?;

        let (reader, writer) = tokio::io::split(stream);

        Ok(Self {
            reader,
            writer,
            next_id: 1,
        })
    }

    /// Send a command and wait for the response
    pub async fn send_command(&mut self, command: Command) -> Result<serde_json::Value> {
        let id = self.next_id;
        self.next_id += 1;

        let request = Request { id, command };
        let json = serde_json::to_vec(&request)?;

        transport::send_message(&mut self.writer, &json)
            .await
            .map_err(|e| Error::DaemonCommunication(e.to_string()))?;

        let response_data = transport::recv_message(&mut self.reader)
            .await
            .map_err(|e| Error::DaemonCommunication(e.to_string()))?;

        let response: Response = serde_json::from_slice(&response_data)?;

        if response.id != id {
            return Err(Error::DaemonCommunication(format!(
                "Response ID mismatch: expected {}, got {}",
                id, response.id
            )));
        }

        if response.success {
            Ok(response.result.unwrap_or(serde_json::json!({})))
        } else {
            let error = response
                .error
                .unwrap_or_else(|| crate::common::error::IpcError {
                    code: "UNKNOWN".to_string(),
                    message: "Unknown error".to_string(),
                });
            Err(error.into())
        }
    }

    /// Check if daemon is responding
    pub async fn ping(&mut self) -> Result<bool> {
        match self.send_command(Command::Status).await {
            Ok(_) => Ok(true),
            Err(Error::DaemonNotRunning) => Ok(false),
            Err(e) => Err(e),
        }
    }
}
