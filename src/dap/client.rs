//! DAP client for communicating with debug adapters
//!
//! This module handles the communication with DAP adapters like lldb-dap,
//! including the initialization sequence and request/response handling.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};

use serde_json::Value;
use tokio::io::{BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};

use crate::common::{Error, Result};

use super::codec;
use super::types::*;

/// DAP client for communicating with a debug adapter
pub struct DapClient {
    /// Adapter subprocess
    adapter: Child,
    /// Buffered reader for adapter stdout
    reader: BufReader<ChildStdout>,
    /// Buffered writer for adapter stdin
    writer: BufWriter<ChildStdin>,
    /// Sequence number for requests
    seq: AtomicI64,
    /// Adapter capabilities (populated after initialize)
    pub capabilities: Capabilities,
    /// Pending requests waiting for responses
    pending: HashMap<i64, oneshot::Sender<ResponseMessage>>,
    /// Channel for events
    event_tx: mpsc::UnboundedSender<Event>,
    /// Receiver for events (given to session)
    event_rx: Option<mpsc::UnboundedReceiver<Event>>,
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

        Ok(Self {
            adapter,
            reader: BufReader::new(stdout),
            writer: BufWriter::new(stdin),
            seq: AtomicI64::new(1),
            capabilities: Capabilities::default(),
            pending: HashMap::new(),
            event_tx,
            event_rx: Some(event_rx),
        })
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
        eprintln!("DAP >>> {}", json);
        tracing::debug!("DAP request: {}", json);

        codec::write_message(&mut self.writer, &json).await?;

        Ok(seq)
    }

    /// Read the next message from the adapter
    async fn read_message(&mut self) -> Result<Value> {
        let json = codec::read_message(&mut self.reader).await?;
        eprintln!("DAP <<< {}", json);
        tracing::debug!("DAP message: {}", json);
        serde_json::from_str(&json).map_err(|e| Error::DapProtocol(format!("Invalid JSON: {}", e)))
    }

    /// Send a request and wait for the response
    ///
    /// This also processes any events that arrive while waiting
    pub async fn request<T: serde::de::DeserializeOwned>(
        &mut self,
        command: &str,
        arguments: Option<Value>,
    ) -> Result<T> {
        let seq = self.send_request(command, arguments).await?;

        // Read messages until we get the response
        loop {
            let msg = self.read_message().await?;

            let msg_type = msg
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            match msg_type {
                "response" => {
                    let response: ResponseMessage = serde_json::from_value(msg)?;

                    if response.request_seq == seq {
                        if response.success {
                            let body = response.body.unwrap_or(Value::Null);
                            return serde_json::from_value(body).map_err(|e| {
                                Error::DapProtocol(format!(
                                    "Failed to parse {} response: {}",
                                    command, e
                                ))
                            });
                        } else {
                            return Err(Error::dap_request_failed(
                                command,
                                &response.message.unwrap_or_else(|| "Unknown error".to_string()),
                            ));
                        }
                    } else if let Some(tx) = self.pending.remove(&response.request_seq) {
                        let _ = tx.send(response);
                    }
                }
                "event" => {
                    let event_msg: EventMessage = serde_json::from_value(msg)?;
                    let event = Event::from_message(&event_msg);
                    let _ = self.event_tx.send(event);
                }
                _ => {
                    tracing::warn!("Unknown message type: {}", msg_type);
                }
            }
        }
    }

    /// Poll for the next event (non-blocking read of buffered events)
    pub async fn poll_event(&mut self) -> Result<Option<Event>> {
        // Try to read a message without blocking indefinitely
        // This is a simple implementation - a full solution would use select! or similar
        tokio::select! {
            biased;
            msg = self.read_message() => {
                let msg = msg?;
                let msg_type = msg.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");

                match msg_type {
                    "event" => {
                        let event_msg: EventMessage = serde_json::from_value(msg)?;
                        Ok(Some(Event::from_message(&event_msg)))
                    }
                    "response" => {
                        // Store for later retrieval
                        let response: ResponseMessage = serde_json::from_value(msg)?;
                        if let Some(tx) = self.pending.remove(&response.request_seq) {
                            let _ = tx.send(response);
                        }
                        Ok(None)
                    }
                    _ => Ok(None)
                }
            }
        }
    }

    /// Initialize the debug adapter
    pub async fn initialize(&mut self, adapter_id: &str) -> Result<Capabilities> {
        let args = InitializeArguments {
            adapter_id: adapter_id.to_string(),
            ..Default::default()
        };

        let caps: Capabilities = self
            .request("initialize", Some(serde_json::to_value(&args)?))
            .await?;

        self.capabilities = caps.clone();
        Ok(caps)
    }

    /// Wait for the initialized event
    pub async fn wait_initialized(&mut self) -> Result<()> {
        loop {
            let msg = self.read_message().await?;

            let msg_type = msg
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");

            if msg_type == "event" {
                let event_msg: EventMessage = serde_json::from_value(msg)?;
                let event = Event::from_message(&event_msg);

                if matches!(event, Event::Initialized) {
                    return Ok(());
                }

                // Forward other events
                let _ = self.event_tx.send(event);
            }
        }
    }

    /// Launch a program for debugging
    pub async fn launch(&mut self, args: LaunchArguments) -> Result<()> {
        self.request::<Value>("launch", Some(serde_json::to_value(&args)?))
            .await?;
        Ok(())
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

    /// Terminate the adapter process
    pub async fn terminate(&mut self) -> Result<()> {
        // Try graceful disconnect first
        let _ = self.disconnect(true).await;

        // Wait a bit for clean shutdown
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Force kill if still running
        let _ = self.adapter.kill().await;

        Ok(())
    }

    /// Check if the adapter is still running
    pub fn is_running(&mut self) -> bool {
        self.adapter.try_wait().ok().flatten().is_none()
    }
}

impl Drop for DapClient {
    fn drop(&mut self) {
        // Try to kill the adapter on drop
        // This is best-effort since we can't await in drop
        let _ = self.adapter.start_kill();
    }
}
