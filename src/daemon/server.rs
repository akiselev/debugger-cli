//! Daemon server - IPC listener and connection tasks
//!
//! The accept loop spawns one task per client connection, so clients are
//! handled concurrently. All session access goes through the session actor
//! (see `actor.rs`); `await` is handled here by waiting on state snapshots so
//! it never blocks other clients.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use interprocess::local_socket::traits::tokio::Listener as ListenerTrait;
use serde_json::json;
use tokio::io::BufReader;
use tokio::sync::{mpsc, oneshot, watch};

use crate::common::{config::Config, error::IpcError, paths, Error, Result};
use crate::ipc::{
    protocol::{Command, Request, Response, StackFrameInfo, StopResult},
    transport,
};

use super::actor::{self, ActorRequest, SessionSnapshot};
use super::session::SessionState;

/// Handles shared by every connection task.
#[derive(Clone)]
struct Shared {
    requests: mpsc::Sender<ActorRequest>,
    snapshots: watch::Receiver<SessionSnapshot>,
    shutdown_tx: Arc<watch::Sender<bool>>,
    shutdown_rx: watch::Receiver<bool>,
    last_activity: Arc<Mutex<Instant>>,
}

/// Main daemon server
pub struct Daemon {
    /// Configuration
    config: Arc<Config>,
}

impl Daemon {
    /// Create a new daemon instance
    pub async fn new() -> Result<Self> {
        let config = Arc::new(Config::load()?);
        Ok(Self { config })
    }

    /// Run the daemon main loop
    pub async fn run(&mut self) -> Result<()> {
        // Create the IPC listener
        let listener = transport::create_listener().await?;
        tracing::info!("Daemon listening on {}", paths::socket_name());

        let idle_timeout = Duration::from_secs(self.config.daemon.idle_timeout_minutes * 60);

        let (request_tx, request_rx) = mpsc::channel(32);
        let (snapshot_tx, snapshot_rx) = watch::channel(SessionSnapshot::default());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let actor_task = tokio::spawn(actor::run(self.config.clone(), request_rx, snapshot_tx));

        let shared = Shared {
            requests: request_tx,
            snapshots: snapshot_rx,
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
            last_activity: Arc::new(Mutex::new(Instant::now())),
        };
        let mut shutdown_rx = shared.shutdown_rx.clone();

        loop {
            // Check for idle timeout
            let idle = !shared.snapshots.borrow().session_active
                && shared.last_activity.lock().unwrap().elapsed() > idle_timeout;
            if idle {
                tracing::info!("Idle timeout reached, shutting down daemon");
                break;
            }

            if *shutdown_rx.borrow() {
                tracing::info!("Shutdown requested, exiting");
                break;
            }

            // Accept connections with timeout, also handle signals
            if run_select_loop(&listener, &shared, &mut shutdown_rx).await? {
                break;
            }
        }

        // Cleanup: tell connection tasks to exit, then let the actor stop the
        // session once every request sender is dropped.
        tracing::info!("Cleaning up daemon resources");
        let _ = shared.shutdown_tx.send(true);
        drop(shared);
        if tokio::time::timeout(Duration::from_secs(10), actor_task)
            .await
            .is_err()
        {
            tracing::warn!("Session actor did not shut down in time");
        }

        // Remove socket file
        paths::remove_socket()?;
        tracing::info!("Daemon shutdown complete");

        Ok(())
    }
}

/// Run one iteration of the select loop, returns true if should break
#[cfg(unix)]
async fn run_select_loop(
    listener: &transport::platform::Listener,
    shared: &Shared,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<bool> {
    use tokio::signal::unix::{signal, SignalKind};

    // Set up signal handlers (recreated each iteration to avoid lifetime issues)
    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to create SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("Failed to create SIGINT handler");

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
        _ = shutdown_rx.changed() => Ok(*shutdown_rx.borrow()),
        accept_result = listener.accept() => {
            match accept_result {
                Ok(stream) => {
                    *shared.last_activity.lock().unwrap() = Instant::now();
                    tokio::spawn(handle_client(stream, shared.clone()));
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
            Ok(false)
        }
        _ = tokio::time::sleep(Duration::from_secs(1)) => {
            // Periodic wakeup to check idle timeout
            Ok(false)
        }
    }
}

/// Run one iteration of the select loop (Windows version)
#[cfg(not(unix))]
async fn run_select_loop(
    listener: &transport::platform::Listener,
    shared: &Shared,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> Result<bool> {
    tokio::select! {
        _ = shutdown_rx.changed() => Ok(*shutdown_rx.borrow()),
        accept_result = listener.accept() => {
            match accept_result {
                Ok(stream) => {
                    *shared.last_activity.lock().unwrap() = Instant::now();
                    tokio::spawn(handle_client(stream, shared.clone()));
                }
                Err(e) => {
                    tracing::error!("Accept error: {}", e);
                }
            }
            Ok(false)
        }
        _ = tokio::time::sleep(Duration::from_secs(1)) => {
            // Periodic wakeup to check idle timeout
            Ok(false)
        }
    }
}

/// Handle a single client connection
async fn handle_client(stream: transport::platform::Stream, mut shared: Shared) {
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
            _ = shared.shutdown_rx.changed() => {
                tracing::debug!("Daemon shutting down, closing client connection");
                break;
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
                    IpcError {
                        code: "INVALID_REQUEST".to_string(),
                        message: e.to_string(),
                    },
                );
                if send_response(&mut writer, &response).await.is_err() {
                    break;
                }
                continue;
            }
        };

        tracing::debug!("Received command: {:?}", request.command);
        *shared.last_activity.lock().unwrap() = Instant::now();

        let mut shutdown_after_reply = false;
        let response = match request.command {
            Command::Shutdown => {
                shutdown_after_reply = true;
                Response::ok(request.id)
            }
            // Await waits on state snapshots so a stopped/exited transition can
            // be observed without occupying the session actor; other clients
            // stay free to send pause/continue while this connection waits.
            Command::Await { timeout_secs } => {
                match await_stop(timeout_secs, &shared).await {
                    Ok(result) => Response::success(request.id, result),
                    Err(e) => Response::error(request.id, IpcError::from(&e)),
                }
            }
            command => dispatch(request.id, command, &shared).await,
        };

        if send_response(&mut writer, &response).await.is_err() {
            break;
        }
        *shared.last_activity.lock().unwrap() = Instant::now();

        if shutdown_after_reply {
            let _ = shared.shutdown_tx.send(true);
            break;
        }
    }
}

async fn send_response(
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    response: &Response,
) -> std::io::Result<()> {
    let json = serde_json::to_vec(response).map_err(std::io::Error::other)?;
    transport::send_message(writer, &json).await
}

/// Forward a command to the session actor and wait for its reply.
async fn dispatch(id: u64, command: Command, shared: &Shared) -> Response {
    let (reply_tx, reply_rx) = oneshot::channel();
    let request = ActorRequest {
        id,
        command,
        reply: reply_tx,
    };

    if shared.requests.send(request).await.is_err() {
        return daemon_stopping_response(id);
    }

    match reply_rx.await {
        Ok(response) => response,
        Err(_) => daemon_stopping_response(id),
    }
}

fn daemon_stopping_response(id: u64) -> Response {
    Response::error(
        id,
        IpcError::from(&Error::Internal("daemon is shutting down".to_string())),
    )
}

/// Wait for the session to stop by watching state snapshots.
async fn await_stop(timeout_secs: u64, shared: &Shared) -> Result<serde_json::Value> {
    let mut snapshots = shared.snapshots.clone();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);

    loop {
        let snapshot = snapshots.borrow_and_update().clone();

        if !snapshot.session_active {
            return Err(Error::SessionNotActive);
        }

        match snapshot.state {
            Some(SessionState::Stopped) => {
                return build_stop_result(&snapshot, shared).await;
            }
            Some(SessionState::Exited) => {
                // Adapters that report an exit code send Exited; a bare
                // Terminated event leaves the code unknown.
                return Ok(match snapshot.exit_code {
                    Some(code) => json!({ "reason": "exited", "exit_code": code }),
                    None => json!({ "reason": "terminated" }),
                });
            }
            _ => {}
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(Error::AwaitTimeout(timeout_secs));
        }

        match tokio::time::timeout(remaining, snapshots.changed()).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                return Err(Error::Internal("daemon is shutting down".to_string()));
            }
            Err(_) => return Err(Error::AwaitTimeout(timeout_secs)),
        }
    }
}

/// Build the stop result for `await`, including the top frame's location.
async fn build_stop_result(
    snapshot: &SessionSnapshot,
    shared: &Shared,
) -> Result<serde_json::Value> {
    let (source, line, column) = fetch_stop_location(shared).await;

    let result = match &snapshot.last_stop {
        Some(body) => StopResult {
            reason: body.reason.clone(),
            description: body.description.clone(),
            thread_id: body.thread_id,
            all_threads_stopped: body.all_threads_stopped,
            hit_breakpoint_ids: body.hit_breakpoint_ids.clone(),
            source,
            line,
            column,
        },
        // Stopped without an adapter event (attach, stop-on-entry).
        None => StopResult {
            reason: snapshot
                .stopped_reason
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            description: None,
            thread_id: snapshot.stopped_thread,
            all_threads_stopped: true,
            hit_breakpoint_ids: vec![],
            source,
            line,
            column,
        },
    };

    Ok(serde_json::to_value(result)?)
}

/// Ask the actor for the top stack frame and extract filename/line/column.
async fn fetch_stop_location(shared: &Shared) -> (Option<String>, Option<u32>, Option<u32>) {
    let response = dispatch(
        0,
        Command::StackTrace {
            thread_id: None,
            limit: 1,
        },
        shared,
    )
    .await;

    let frames: Vec<StackFrameInfo> = match response
        .result
        .and_then(|mut r| r.get_mut("frames").map(serde_json::Value::take))
        .map(serde_json::from_value)
    {
        Some(Ok(frames)) if response.success => frames,
        _ => return (None, None, None),
    };

    let Some(frame) = frames.first() else {
        return (None, None, None);
    };

    // Report just the filename, matching the pre-actor await output.
    let source = frame.source.as_ref().map(|path| {
        std::path::Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path)
            .to_string()
    });

    (source, frame.line, frame.column)
}
