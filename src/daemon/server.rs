//! Daemon server - IPC listener and main event loop

use std::time::{Duration, Instant};

use interprocess::local_socket::traits::tokio::Listener as ListenerTrait;
use tokio::io::BufReader;

use crate::common::{config::Config, paths, Result};
use crate::ipc::{
    protocol::{Command, Request, Response},
    transport,
};

use super::handler;
use super::session::DebugSession;

/// Main daemon server
pub struct Daemon {
    /// Configuration
    config: Config,
    /// Active debug session
    session: Option<DebugSession>,
    /// Last activity timestamp for idle timeout
    last_activity: Instant,
    /// Whether shutdown was requested
    shutdown_requested: bool,
}

impl Daemon {
    /// Create a new daemon instance
    pub async fn new() -> Result<Self> {
        let config = Config::load()?;

        Ok(Self {
            config,
            session: None,
            last_activity: Instant::now(),
            shutdown_requested: false,
        })
    }

    /// Run the daemon main loop
    pub async fn run(&mut self) -> Result<()> {
        // Create the IPC listener
        let listener = transport::create_listener().await?;
        tracing::info!("Daemon listening on {}", paths::socket_name());

        let idle_timeout = Duration::from_secs(self.config.daemon.idle_timeout_minutes * 60);

        loop {
            // Check for idle timeout
            if self.session.is_none() && self.last_activity.elapsed() > idle_timeout {
                tracing::info!("Idle timeout reached, shutting down daemon");
                break;
            }

            // Check for shutdown request
            if self.shutdown_requested {
                tracing::info!("Shutdown requested, exiting");
                break;
            }

            // Accept connections with timeout, also handle signals
            if self.run_select_loop(&listener).await? {
                break;
            }
        }

        // Cleanup
        tracing::info!("Cleaning up daemon resources");
        if let Some(mut session) = self.session.take() {
            tracing::debug!("Stopping debug session");
            let _ = session.stop().await;
        }

        // Remove socket file
        paths::remove_socket()?;
        tracing::info!("Daemon shutdown complete");

        Ok(())
    }

    /// Run one iteration of the select loop, returns true if should break
    #[cfg(unix)]
    async fn run_select_loop(
        &mut self,
        listener: &transport::platform::Listener,
    ) -> Result<bool> {
        use tokio::signal::unix::{signal, SignalKind};
        
        // Set up signal handlers (recreated each iteration to avoid lifetime issues)
        let mut sigterm = signal(SignalKind::terminate())
            .expect("Failed to create SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt())
            .expect("Failed to create SIGINT handler");

        tokio::select! {
            // Handle SIGTERM (graceful shutdown)
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM, shutting down gracefully");
                Ok(true)
            }
            // Handle SIGINT (Ctrl+C)
            _ = sigint.recv() => {
                tracing::info!("Received SIGINT (Ctrl+C), shutting down gracefully");
                Ok(true)
            }
            accept_result = listener.accept() => {
                match accept_result {
                    Ok(stream) => {
                        self.last_activity = Instant::now();
                        if let Err(e) = self.handle_client(stream).await {
                            tracing::error!("Error handling client: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Accept error: {}", e);
                    }
                }
                Ok(false)
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                // Periodic wakeup to check idle timeout
                // Also process any pending events
                if let Some(session) = &mut self.session {
                    if let Err(e) = session.process_events().await {
                        tracing::warn!("Error processing events: {}", e);
                    }
                }
                Ok(false)
            }
        }
    }

    /// Run one iteration of the select loop (Windows version)
    #[cfg(not(unix))]
    async fn run_select_loop(
        &mut self,
        listener: &transport::platform::Listener,
    ) -> Result<bool> {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok(stream) => {
                        self.last_activity = Instant::now();
                        if let Err(e) = self.handle_client(stream).await {
                            tracing::error!("Error handling client: {}", e);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Accept error: {}", e);
                    }
                }
                Ok(false)
            }
            _ = tokio::time::sleep(Duration::from_secs(1)) => {
                // Periodic wakeup to check idle timeout
                if let Some(session) = &mut self.session {
                    if let Err(e) = session.process_events().await {
                        tracing::warn!("Error processing events: {}", e);
                    }
                }
                Ok(false)
            }
        }
    }

    /// Handle a single client connection
    async fn handle_client(
        &mut self,
        stream: transport::platform::Stream,
    ) -> Result<()> {
        let (reader, mut writer) = tokio::io::split(stream);
        let mut reader = BufReader::new(reader);

        // Read and process commands until client disconnects
        loop {
            // Read request with timeout
            let request_data = tokio::select! {
                result = transport::recv_message(&mut reader) => {
                    match result {
                        Ok(data) => data,
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                            // Client disconnected
                            tracing::debug!("Client disconnected");
                            break;
                        }
                        Err(e) => {
                            tracing::error!("Error reading request: {}", e);
                            break;
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(300)) => {
                    // Client timeout
                    tracing::debug!("Client timeout");
                    break;
                }
            };

            // Parse request
            let request: Request = match serde_json::from_slice(&request_data) {
                Ok(req) => req,
                Err(e) => {
                    tracing::error!("Invalid request: {}", e);
                    let response = Response::error(
                        0,
                        crate::common::error::IpcError {
                            code: "INVALID_REQUEST".to_string(),
                            message: e.to_string(),
                        },
                    );
                    let json = serde_json::to_vec(&response)?;
                    transport::send_message(&mut writer, &json).await?;
                    continue;
                }
            };

            tracing::debug!("Received command: {:?}", request.command);

            // Check for shutdown command
            if matches!(request.command, Command::Shutdown) {
                self.shutdown_requested = true;
                let response = Response::ok(request.id);
                let json = serde_json::to_vec(&response)?;
                transport::send_message(&mut writer, &json).await?;
                break;
            }

            // Handle command
            let response = handler::handle_command(
                &mut self.session,
                &self.config,
                request.id,
                request.command,
            )
            .await;

            // Send response
            let json = serde_json::to_vec(&response)?;
            transport::send_message(&mut writer, &json).await?;

            self.last_activity = Instant::now();
        }

        Ok(())
    }
}
