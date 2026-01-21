//! DAP client for communicating with debug adapters
//!
//! This module handles the communication with DAP adapters like lldb-dap,
//! including the initialization sequence and request/response handling.
//!
//! ## Architecture
//!
//! The DapClient uses a background reader task to continuously read from the
//! adapter's stdout (or TCP socket). This ensures that events (stopped, output,
//! etc.) are captured immediately rather than only during request/response cycles.
//!
//! ```text
//! [DAP Adapter] --stdout/tcp--> [Reader Task] --events--> [event_tx channel]
//!                                            --responses-> [response channels]
//! [DapClient]   --stdin/tcp-->  [DAP Adapter]
//! ```
//!
//! ## Transport Modes
//!
//! - **Stdio**: Standard input/output (default, used by lldb-dap, debugpy)
//! - **TCP**: TCP socket connection (used by Delve)

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use std::pin::Pin;
use std::task::{Context, Poll};

use serde_json::Value;
use tokio::io::{AsyncWrite, BufReader, BufWriter};
use tokio::net::TcpStream;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

use crate::common::{Error, Result};

use super::codec;
use super::types::*;

/// Pending response waiters, keyed by request sequence number
type PendingResponses = Arc<Mutex<HashMap<i64, oneshot::Sender<std::result::Result<ResponseMessage, Error>>>>>;

/// Abstraction over different writer types (stdin or TCP)
enum DapWriter {
    Stdio(BufWriter<ChildStdin>),
    Tcp(BufWriter<tokio::io::WriteHalf<TcpStream>>),
}

impl AsyncWrite for DapWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            DapWriter::Stdio(w) => Pin::new(w).poll_write(cx, buf),
            DapWriter::Tcp(w) => Pin::new(w).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            DapWriter::Stdio(w) => Pin::new(w).poll_flush(cx),
            DapWriter::Tcp(w) => Pin::new(w).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            DapWriter::Stdio(w) => Pin::new(w).poll_shutdown(cx),
            DapWriter::Tcp(w) => Pin::new(w).poll_shutdown(cx),
        }
    }
}

/// DAP client for communicating with a debug adapter
pub struct DapClient {
    /// Adapter subprocess
    adapter: Child,
    /// Buffered writer for adapter communication
    writer: DapWriter,
    /// Sequence number for requests
    seq: AtomicI64,
    /// Adapter capabilities (populated after initialize)
    pub capabilities: Capabilities,
    /// Pending response waiters
    pending: PendingResponses,
    /// Channel for events (to session)
    event_tx: mpsc::UnboundedSender<Event>,
    /// Receiver for events (given to session)
    event_rx: Option<mpsc::UnboundedReceiver<Event>>,
    /// Handle to the background reader task
    reader_task: Option<tokio::task::JoinHandle<()>>,
    /// Channel to signal reader task to stop
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl DapClient {
    /// Spawn a new DAP adapter and create a client
    pub async fn spawn(adapter_path: &Path, args: &[String]) -> Result<Self> {
        let mut cmd = Command::new(adapter_path);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // Let adapter errors go to stderr

        let mut adapter = cmd.spawn().map_err(|e| {
            Error::AdapterStartFailed(format!(
                "Failed to start {}: {}",
                adapter_path.display(),
                e
            ))
        })?;

        let stdin = adapter
            .stdin
            .take()
            .ok_or_else(|| Error::AdapterStartFailed("Failed to get adapter stdin".to_string()))?;
        let stdout = adapter.stdout.take().ok_or_else(|| {
            Error::AdapterStartFailed("Failed to get adapter stdout".to_string())
        })?;

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let pending: PendingResponses = Arc::new(Mutex::new(HashMap::new()));

        // Spawn background reader task
        let reader_task = Self::spawn_stdio_reader_task(
            stdout,
            event_tx.clone(),
            pending.clone(),
            shutdown_rx,
        );

        Ok(Self {
            adapter,
            writer: DapWriter::Stdio(BufWriter::new(stdin)),
            seq: AtomicI64::new(1),
            capabilities: Capabilities::default(),
            pending,
            event_tx,
            event_rx: Some(event_rx),
            reader_task: Some(reader_task),
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Spawn a new DAP adapter that uses TCP for communication (e.g., Delve)
    ///
    /// This spawns the adapter with a --listen flag, waits for it to output
    /// the port it's listening on, then connects via TCP.
    pub async fn spawn_tcp(adapter_path: &Path, args: &[String]) -> Result<Self> {
        use tokio::io::{AsyncBufReadExt, BufReader as TokioBufReader};

        // Build command with --listen=127.0.0.1:0 to get a random available port
        let mut cmd = Command::new(adapter_path);
        cmd.args(args)
            .arg("--listen=127.0.0.1:0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut adapter = cmd.spawn().map_err(|e| {
            Error::AdapterStartFailed(format!(
                "Failed to start {}: {}",
                adapter_path.display(),
                e
            ))
        })?;

        // Read stdout to find the listening address
        // Delve outputs: "DAP server listening at: 127.0.0.1:PORT"
        let stdout = adapter.stdout.take().ok_or_else(|| {
            Error::AdapterStartFailed("Failed to get adapter stdout".to_string())
        })?;

        let mut stdout_reader = TokioBufReader::new(stdout);
        let mut line = String::new();

        // Wait for the "listening at" message with timeout
        let addr = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                line.clear();
                let bytes_read = stdout_reader.read_line(&mut line).await.map_err(|e| {
                    Error::AdapterStartFailed(format!("Failed to read adapter output: {}", e))
                })?;

                if bytes_read == 0 {
                    return Err(Error::AdapterStartFailed(
                        "Adapter exited before outputting listen address".to_string(),
                    ));
                }

                tracing::debug!("Delve output: {}", line.trim());

                // Look for the listening address in the output
                // Format: "DAP server listening at: 127.0.0.1:PORT" or "[::]:PORT"
                if let Some(addr_start) = line.find("listening at:") {
                    let addr_part = &line[addr_start + "listening at:".len()..];
                    let addr = addr_part.trim().to_string();
                    // Handle IPv6 format [::]:PORT
                    let addr = if addr.starts_with("[::]:") {
                        addr.replace("[::]:", "127.0.0.1:")
                    } else {
                        addr
                    };
                    return Ok(addr);
                }
            }
        })
        .await
        .map_err(|_| {
            Error::AdapterStartFailed(
                "Timeout waiting for Delve to start listening".to_string(),
            )
        })??;

        tracing::info!("Connecting to Delve DAP server at {}", addr);

        // Connect to the TCP port
        let stream = TcpStream::connect(&addr).await.map_err(|e| {
            Error::AdapterStartFailed(format!("Failed to connect to Delve at {}: {}", addr, e))
        })?;

        let (read_half, write_half) = tokio::io::split(stream);

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        let pending: PendingResponses = Arc::new(Mutex::new(HashMap::new()));

        // Spawn background reader task for TCP
        let reader_task = Self::spawn_tcp_reader_task(
            read_half,
            event_tx.clone(),
            pending.clone(),
            shutdown_rx,
        );

        Ok(Self {
            adapter,
            writer: DapWriter::Tcp(BufWriter::new(write_half)),
            seq: AtomicI64::new(1),
            capabilities: Capabilities::default(),
            pending,
            event_tx,
            event_rx: Some(event_rx),
            reader_task: Some(reader_task),
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Spawn the background reader task for stdio-based adapters
    fn spawn_stdio_reader_task(
        stdout: ChildStdout,
        event_tx: mpsc::UnboundedSender<Event>,
        pending: PendingResponses,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);

            loop {
                tokio::select! {
                    biased;

                    // Check for shutdown signal
                    _ = shutdown_rx.recv() => {
                        tracing::debug!("Reader task received shutdown signal");
                        break;
                    }

                    // Read next message
                    result = codec::read_message(&mut reader) => {
                        match result {
                            Ok(json) => {
                                tracing::trace!("DAP <<< {}", json);

                                if let Err(e) = Self::process_message(&json, &event_tx, &pending).await {
                                    tracing::error!("Error processing DAP message: {}", e);
                                }
                            }
                            Err(e) => {
                                // Check if this is an expected EOF (adapter exited)
                                // We check the error message string as a fallback for various error types
                                let err_str = e.to_string().to_lowercase();
                                let is_eof = err_str.contains("unexpected eof")
                                    || err_str.contains("unexpectedeof")
                                    || err_str.contains("end of file");

                                if is_eof {
                                    tracing::info!("DAP adapter closed connection");
                                } else {
                                    tracing::error!("Error reading from DAP adapter: {}", e);
                                }

                                // Signal error to any pending requests
                                let mut pending_guard = pending.lock().await;
                                for (_, tx) in pending_guard.drain() {
                                    let _ = tx.send(Err(Error::AdapterCrashed));
                                }

                                // Send terminated event to notify the session
                                let _ = event_tx.send(Event::Terminated(None));
                                break;
                            }
                        }
                    }
                }
            }

            tracing::debug!("Reader task exiting");
        })
    }

    /// Spawn the background reader task for TCP-based adapters
    fn spawn_tcp_reader_task(
        read_half: tokio::io::ReadHalf<TcpStream>,
        event_tx: mpsc::UnboundedSender<Event>,
        pending: PendingResponses,
        mut shutdown_rx: mpsc::Receiver<()>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);

            loop {
                tokio::select! {
                    biased;

                    // Check for shutdown signal
                    _ = shutdown_rx.recv() => {
                        tracing::debug!("TCP reader task received shutdown signal");
                        break;
                    }

                    // Read next message
                    result = codec::read_message(&mut reader) => {
                        match result {
                            Ok(json) => {
                                tracing::trace!("DAP <<< {}", json);

                                if let Err(e) = Self::process_message(&json, &event_tx, &pending).await {
                                    tracing::error!("Error processing DAP message: {}", e);
                                }
                            }
                            Err(e) => {
                                let err_str = e.to_string().to_lowercase();
                                let is_eof = err_str.contains("unexpected eof")
                                    || err_str.contains("unexpectedeof")
                                    || err_str.contains("end of file")
                                    || err_str.contains("connection reset");

                                if is_eof {
                                    tracing::info!("DAP adapter closed TCP connection");
                                } else {
                                    tracing::error!("Error reading from DAP adapter (TCP): {}", e);
                                }

                                // Signal error to any pending requests
                                let mut pending_guard = pending.lock().await;
                                for (_, tx) in pending_guard.drain() {
                                    let _ = tx.send(Err(Error::AdapterCrashed));
                                }

                                // Send terminated event to notify the session
                                let _ = event_tx.send(Event::Terminated(None));
                                break;
                            }
                        }
                    }
                }
            }

            tracing::debug!("TCP reader task exiting");
        })
    }

    /// Process a single message from the adapter
    async fn process_message(
        json: &str,
        event_tx: &mpsc::UnboundedSender<Event>,
        pending: &PendingResponses,
    ) -> Result<()> {
        let msg: Value = serde_json::from_str(json)
            .map_err(|e| Error::DapProtocol(format!("Invalid JSON: {}", e)))?;

        let msg_type = msg
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        match msg_type {
            "response" => {
                let response: ResponseMessage = serde_json::from_value(msg)?;
                let seq = response.request_seq;

                let mut pending_guard = pending.lock().await;
                if let Some(tx) = pending_guard.remove(&seq) {
                    let _ = tx.send(Ok(response));
                } else {
                    tracing::warn!("Received response for unknown request seq {}", seq);
                }
            }
            "event" => {
                let event_msg: EventMessage = serde_json::from_value(msg)?;
                let event = Event::from_message(&event_msg);
                let _ = event_tx.send(event);
            }
            _ => {
                tracing::warn!("Unknown message type: {}", msg_type);
            }
        }

        Ok(())
    }

    /// Take the event receiver (can only be called once)
    pub fn take_event_receiver(&mut self) -> Option<mpsc::UnboundedReceiver<Event>> {
        self.event_rx.take()
    }

    /// Get the next sequence number
    fn next_seq(&self) -> i64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }

    /// Send a request and return its sequence number
    async fn send_request(&mut self, command: &str, arguments: Option<Value>) -> Result<i64> {
        let seq = self.next_seq();

        // Build request with or without arguments field
        let request = if let Some(args) = arguments {
            serde_json::json!({
                "seq": seq,
                "type": "request",
                "command": command,
                "arguments": args
            })
        } else {
            serde_json::json!({
                "seq": seq,
                "type": "request",
                "command": command
            })
        };

        let json = serde_json::to_string(&request)?;
        tracing::trace!("DAP >>> {}", json);

        codec::write_message(&mut self.writer, &json).await?;

        Ok(seq)
    }

    /// Send a request and wait for the response with timeout
    pub async fn request<T: serde::de::DeserializeOwned>(
        &mut self,
        command: &str,
        arguments: Option<Value>,
    ) -> Result<T> {
        self.request_with_timeout(command, arguments, Duration::from_secs(30)).await
    }

    /// Send a request and wait for the response with configurable timeout
    ///
    /// Note: We register the pending response handler BEFORE sending the request
    /// to avoid a race condition where a fast adapter response arrives before
    /// we've set up the handler.
    pub async fn request_with_timeout<T: serde::de::DeserializeOwned>(
        &mut self,
        command: &str,
        arguments: Option<Value>,
        timeout: Duration,
    ) -> Result<T> {
        let seq = self.next_seq();

        // Build request with or without arguments field
        let request = if let Some(ref args) = arguments {
            serde_json::json!({
                "seq": seq,
                "type": "request",
                "command": command,
                "arguments": args
            })
        } else {
            serde_json::json!({
                "seq": seq,
                "type": "request",
                "command": command
            })
        };

        // IMPORTANT: Register the pending response handler BEFORE sending the request
        // to avoid race condition where fast adapter responds before we're ready
        let (tx, rx) = oneshot::channel();
        {
            let mut pending_guard = self.pending.lock().await;
            pending_guard.insert(seq, tx);
        }

        // Now send the request
        let json = serde_json::to_string(&request)?;
        tracing::trace!("DAP >>> {}", json);

        if let Err(e) = codec::write_message(&mut self.writer, &json).await {
            // Remove the pending handler if send failed
            let mut pending_guard = self.pending.lock().await;
            pending_guard.remove(&seq);
            return Err(e);
        }

        // Wait for response with timeout
        let response = tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| {
                // Clean up pending handler on timeout
                let pending = self.pending.clone();
                tokio::spawn(async move {
                    let mut pending_guard = pending.lock().await;
                    pending_guard.remove(&seq);
                });
                Error::Timeout(timeout.as_secs())
            })?
            .map_err(|_| Error::AdapterCrashed)??;

        if response.success {
            let body = response.body.unwrap_or(Value::Null);
            serde_json::from_value(body).map_err(|e| {
                Error::DapProtocol(format!(
                    "Failed to parse {} response: {}",
                    command, e
                ))
            })
        } else {
            Err(Error::dap_request_failed(
                command,
                &response.message.unwrap_or_else(|| "Unknown error".to_string()),
            ))
        }
    }

    /// Poll for events - this is now non-blocking since events are already in the channel
    /// Note: This method is kept for API compatibility but is no longer necessary
    /// since the background reader task handles all event ingestion
    pub async fn poll_event(&mut self) -> Result<Option<Event>> {
        // Events are now handled by the background reader task
        // and delivered through the event channel.
        // This method is kept for backward compatibility.
        Ok(None)
    }

    /// Initialize the debug adapter
    pub async fn initialize(&mut self, adapter_id: &str) -> Result<Capabilities> {
        self.initialize_with_timeout(adapter_id, Duration::from_secs(10)).await
    }

    /// Initialize the debug adapter with configurable timeout
    pub async fn initialize_with_timeout(
        &mut self,
        adapter_id: &str,
        timeout: Duration,
    ) -> Result<Capabilities> {
        let args = InitializeArguments {
            adapter_id: adapter_id.to_string(),
            ..Default::default()
        };

        let caps: Capabilities = self
            .request_with_timeout("initialize", Some(serde_json::to_value(&args)?), timeout)
            .await?;

        self.capabilities = caps.clone();
        Ok(caps)
    }

    /// Wait for the initialized event with timeout
    ///
    /// This method waits for the initialized event which comes through the event channel.
    /// It's called before the session takes the event receiver.
    pub async fn wait_initialized(&mut self) -> Result<()> {
        self.wait_initialized_with_timeout(Duration::from_secs(30)).await
    }

    /// Wait for the initialized event with configurable timeout
    ///
    /// ## Event Ordering Note
    ///
    /// This method consumes events from the channel until it sees `Initialized`.
    /// Non-Initialized events are re-sent to the channel so they won't be lost.
    /// This is safe because:
    /// 1. The session hasn't taken the receiver yet (wait_initialized is called during setup)
    /// 2. The re-sent events go back to the same unbounded channel
    /// 3. The background reader task continues adding new events after our re-sent ones
    ///
    /// Events will be received in order: [re-sent events] + [new events from reader]
    pub async fn wait_initialized_with_timeout(&mut self, timeout: Duration) -> Result<()> {
        // The event receiver is typically taken by the session after initialization,
        // but wait_initialized is called before that, so we should still have it
        if let Some(ref mut rx) = self.event_rx {
            let deadline = tokio::time::Instant::now() + timeout;

            loop {
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    return Err(Error::Timeout(timeout.as_secs()));
                }

                match tokio::time::timeout(remaining, rx.recv()).await {
                    Ok(Some(event)) => {
                        if matches!(event, Event::Initialized) {
                            return Ok(());
                        }
                        // Re-send other events so they're not lost when session takes the receiver.
                        // This maintains event ordering: these events arrived before Initialized,
                        // so they'll be received first when the session starts processing.
                        let _ = self.event_tx.send(event);
                    }
                    Ok(None) => {
                        return Err(Error::AdapterCrashed);
                    }
                    Err(_) => {
                        return Err(Error::Timeout(timeout.as_secs()));
                    }
                }
            }
        } else {
            // Event receiver already taken - this shouldn't happen in normal flow
            Err(Error::Internal("Event receiver already taken before wait_initialized".to_string()))
        }
    }

    /// Launch a program for debugging
    pub async fn launch(&mut self, args: LaunchArguments) -> Result<()> {
        self.request::<Value>("launch", Some(serde_json::to_value(&args)?))
            .await?;
        Ok(())
    }

    /// Launch a program for debugging without waiting for response
    /// 
    /// Some debuggers (like debugpy) don't respond to launch until after
    /// configurationDone is sent. This sends the launch request but doesn't
    /// wait for the response.
    pub async fn launch_no_wait(&mut self, args: LaunchArguments) -> Result<i64> {
        self.send_request("launch", Some(serde_json::to_value(&args)?)).await
    }

    /// Attach to a running process
    pub async fn attach(&mut self, args: AttachArguments) -> Result<()> {
        self.request::<Value>("attach", Some(serde_json::to_value(&args)?))
            .await?;
        Ok(())
    }

    /// Signal that configuration is done
    pub async fn configuration_done(&mut self) -> Result<()> {
        self.request::<Value>("configurationDone", None).await?;
        Ok(())
    }

    /// Set breakpoints for a source file
    pub async fn set_breakpoints(
        &mut self,
        source_path: &Path,
        breakpoints: Vec<SourceBreakpoint>,
    ) -> Result<Vec<Breakpoint>> {
        let args = SetBreakpointsArguments {
            source: Source {
                path: Some(source_path.to_string_lossy().into_owned()),
                ..Default::default()
            },
            breakpoints,
        };

        let response: SetBreakpointsResponseBody = self
            .request("setBreakpoints", Some(serde_json::to_value(&args)?))
            .await?;

        Ok(response.breakpoints)
    }

    /// Set function breakpoints
    pub async fn set_function_breakpoints(
        &mut self,
        breakpoints: Vec<FunctionBreakpoint>,
    ) -> Result<Vec<Breakpoint>> {
        let args = SetFunctionBreakpointsArguments { breakpoints };

        let response: SetBreakpointsResponseBody = self
            .request(
                "setFunctionBreakpoints",
                Some(serde_json::to_value(&args)?),
            )
            .await?;

        Ok(response.breakpoints)
    }

    /// Continue execution
    pub async fn continue_execution(&mut self, thread_id: i64) -> Result<bool> {
        let args = ContinueArguments {
            thread_id,
            single_thread: false,
        };

        let response: ContinueResponseBody = self
            .request("continue", Some(serde_json::to_value(&args)?))
            .await?;

        Ok(response.all_threads_continued)
    }

    /// Step over (next)
    pub async fn next(&mut self, thread_id: i64) -> Result<()> {
        let args = StepArguments {
            thread_id,
            granularity: Some("statement".to_string()),
        };

        self.request::<Value>("next", Some(serde_json::to_value(&args)?))
            .await?;
        Ok(())
    }

    /// Step into
    pub async fn step_in(&mut self, thread_id: i64) -> Result<()> {
        let args = StepArguments {
            thread_id,
            granularity: Some("statement".to_string()),
        };

        self.request::<Value>("stepIn", Some(serde_json::to_value(&args)?))
            .await?;
        Ok(())
    }

    /// Step out
    pub async fn step_out(&mut self, thread_id: i64) -> Result<()> {
        let args = StepArguments {
            thread_id,
            granularity: Some("statement".to_string()),
        };

        self.request::<Value>("stepOut", Some(serde_json::to_value(&args)?))
            .await?;
        Ok(())
    }

    /// Pause execution
    pub async fn pause(&mut self, thread_id: i64) -> Result<()> {
        let args = PauseArguments { thread_id };

        self.request::<Value>("pause", Some(serde_json::to_value(&args)?))
            .await?;
        Ok(())
    }

    /// Get stack trace
    pub async fn stack_trace(&mut self, thread_id: i64, levels: i64) -> Result<Vec<StackFrame>> {
        let args = StackTraceArguments {
            thread_id,
            start_frame: Some(0),
            levels: Some(levels),
        };

        let response: StackTraceResponseBody = self
            .request("stackTrace", Some(serde_json::to_value(&args)?))
            .await?;

        Ok(response.stack_frames)
    }

    /// Get threads
    pub async fn threads(&mut self) -> Result<Vec<Thread>> {
        let response: ThreadsResponseBody = self.request("threads", None).await?;
        Ok(response.threads)
    }

    /// Get scopes for a frame
    pub async fn scopes(&mut self, frame_id: i64) -> Result<Vec<Scope>> {
        let args = ScopesArguments { frame_id };

        let response: ScopesResponseBody = self
            .request("scopes", Some(serde_json::to_value(&args)?))
            .await?;

        Ok(response.scopes)
    }

    /// Get variables
    pub async fn variables(&mut self, variables_reference: i64) -> Result<Vec<Variable>> {
        let args = VariablesArguments {
            variables_reference,
            start: None,
            count: None,
        };

        let response: VariablesResponseBody = self
            .request("variables", Some(serde_json::to_value(&args)?))
            .await?;

        Ok(response.variables)
    }

    /// Evaluate an expression
    pub async fn evaluate(
        &mut self,
        expression: &str,
        frame_id: Option<i64>,
        context: &str,
    ) -> Result<EvaluateResponseBody> {
        let args = EvaluateArguments {
            expression: expression.to_string(),
            frame_id,
            context: Some(context.to_string()),
        };

        self.request("evaluate", Some(serde_json::to_value(&args)?))
            .await
    }

    /// Disconnect from the debug adapter
    pub async fn disconnect(&mut self, terminate_debuggee: bool) -> Result<()> {
        let args = DisconnectArguments {
            restart: false,
            terminate_debuggee: Some(terminate_debuggee),
        };

        // Don't wait for response - adapter might exit immediately
        let _ = self
            .send_request("disconnect", Some(serde_json::to_value(&args)?))
            .await;

        Ok(())
    }

    /// Terminate the adapter process and clean up resources
    pub async fn terminate(&mut self) -> Result<()> {
        // Try graceful disconnect first
        let _ = self.disconnect(true).await;

        // Signal the reader task to stop
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }

        // Wait a bit for clean shutdown
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Wait for reader task to finish
        if let Some(task) = self.reader_task.take() {
            // Give it a short timeout
            let _ = tokio::time::timeout(
                Duration::from_millis(500),
                task,
            ).await;
        }

        // Force kill if still running
        let _ = self.adapter.kill().await;

        Ok(())
    }

    /// Check if the adapter is still running
    pub fn is_running(&mut self) -> bool {
        self.adapter.try_wait().ok().flatten().is_none()
    }

    /// Restart the debug session (for adapters that support it)
    pub async fn restart(&mut self, no_debug: bool) -> Result<()> {
        if !self.capabilities.supports_restart_request {
            return Err(Error::Internal(
                "Debug adapter does not support restart".to_string(),
            ));
        }

        let args = serde_json::json!({
            "noDebug": no_debug
        });

        self.request::<Value>("restart", Some(args)).await?;
        Ok(())
    }
}

impl Drop for DapClient {
    /// Best-effort cleanup on drop.
    ///
    /// ## Limitations
    ///
    /// Since we can't await in `drop()`, this is necessarily imperfect:
    /// - `try_send` may fail if the shutdown channel is full (unlikely with capacity 1)
    /// - `task.abort()` is immediate; the reader may be mid-operation
    /// - `start_kill()` is non-blocking; the adapter may not exit immediately
    ///
    /// For graceful cleanup, prefer calling `terminate()` before dropping.
    /// This Drop impl exists as a safety net to avoid leaking resources if
    /// `terminate()` wasn't called.
    fn drop(&mut self) {
        // Signal shutdown to reader task (best-effort, can't await)
        if let Some(tx) = self.shutdown_tx.take() {
            // Use try_send since we can't await in drop
            let _ = tx.try_send(());
        }

        // Abort the reader task if it's still running
        // Note: This is abrupt but necessary since we can't await graceful shutdown
        if let Some(task) = self.reader_task.take() {
            task.abort();
        }

        // Try to kill the adapter on drop
        // This is best-effort since we can't await in drop
        let _ = self.adapter.start_kill();
    }
}
