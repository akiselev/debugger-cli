//! Debug session state machine
//!
//! Manages the lifecycle of a debug session from initialization through
//! termination.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use crate::common::{config::{Config, TransportMode}, Error, Result};
use crate::dap::{
    self, Breakpoint, Capabilities, DapClient, Event, FunctionBreakpoint, LaunchArguments,
    AttachArguments, Scope, SourceBreakpoint, StackFrame, Thread, Variable,
};
use crate::ipc::protocol::{BreakpointInfo, BreakpointLocation};

/// Debug session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// No active session
    Idle,
    /// DAP adapter starting
    Initializing,
    /// Setting initial breakpoints
    Configuring,
    /// Program is running
    Running,
    /// Program has stopped (breakpoint, step, exception)
    Stopped,
    /// Program has exited
    Exited,
    /// Session is terminating
    Terminating,
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Initializing => write!(f, "initializing"),
            Self::Configuring => write!(f, "configuring"),
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::Exited => write!(f, "exited"),
            Self::Terminating => write!(f, "terminating"),
        }
    }
}

/// Stored breakpoint information
#[derive(Debug, Clone)]
struct StoredBreakpoint {
    id: u32,
    location: BreakpointLocation,
    condition: Option<String>,
    hit_count: Option<u32>,
    enabled: bool,
    verified: bool,
    actual_line: Option<u32>,
    message: Option<String>,
}

/// Output event for buffering
#[derive(Debug, Clone)]
pub struct OutputEvent {
    pub category: String,
    pub output: String,
    pub timestamp: std::time::Instant,
}

/// Debug session managing a DAP connection
pub struct DebugSession {
    /// DAP client connection
    client: DapClient,
    /// Event receiver from DAP client
    events_rx: mpsc::UnboundedReceiver<Event>,
    /// Current session state
    state: SessionState,
    /// Adapter capabilities
    capabilities: Capabilities,
    /// Program being debugged
    program: PathBuf,
    /// Program arguments
    args: Vec<String>,
    /// Adapter name
    adapter_name: String,
    /// Whether we launched (vs attached)
    launched: bool,
    /// All breakpoints by source file
    source_breakpoints: HashMap<PathBuf, Vec<StoredBreakpoint>>,
    /// Function breakpoints
    function_breakpoints: Vec<StoredBreakpoint>,
    /// Next breakpoint ID
    next_bp_id: u32,
    /// Cached threads
    threads: Vec<Thread>,
    /// Currently selected thread (may differ from stopped thread)
    selected_thread: Option<i64>,
    /// Currently stopped thread
    stopped_thread: Option<i64>,
    /// Reason for last stop
    stopped_reason: Option<String>,
    /// Hit breakpoint IDs from last stop
    hit_breakpoints: Vec<u32>,
    /// Current frame index (0 = top of stack)
    current_frame_index: usize,
    /// Current frame ID (for variable inspection)
    current_frame: Option<i64>,
    /// Cached stack frames for current stop
    cached_frames: Vec<StackFrame>,
    /// Output buffer
    output_buffer: VecDeque<OutputEvent>,
    /// Maximum output buffer size
    max_output_events: usize,
    /// Maximum output buffer bytes
    max_output_bytes: usize,
    /// Current output buffer byte count
    current_output_bytes: usize,
    /// Exit code if program exited
    exit_code: Option<i32>,
    /// DAP request timeout
    dap_request_timeout: std::time::Duration,
}

impl DebugSession {
    /// Create a new debug session by launching a program
    #[tracing::instrument(skip(config), fields(adapter = %adapter_name.as_deref().unwrap_or("default")))]
    pub async fn launch(
        config: &Config,
        program: &Path,
        args: Vec<String>,
        adapter_name: Option<String>,
        stop_on_entry: bool,
        initial_breakpoints: Vec<String>,
    ) -> Result<Self> {
        let adapter_name = adapter_name.unwrap_or_else(|| config.defaults.adapter.clone());

        let adapter_config = config.get_adapter(&adapter_name).ok_or_else(|| {
            Error::adapter_not_found(&adapter_name, &[&adapter_name])
        })?;

        tracing::info!(
            program = %program.display(),
            adapter = %adapter_name,
            adapter_path = %adapter_config.path.display(),
            adapter_args = ?adapter_config.args,
            transport = ?adapter_config.transport,
            stop_on_entry,
            "Launching debug session"
        );

        tracing::debug!("Spawning DAP adapter process");
        let mut client = match adapter_config.transport {
            TransportMode::Stdio => {
                DapClient::spawn(&adapter_config.path, &adapter_config.args).await?
            }
            TransportMode::Tcp => {
                DapClient::spawn_tcp(&adapter_config.path, &adapter_config.args).await?
            }
        };

        // Take the event receiver
        // Initialize the adapter with timeout
        let init_timeout = std::time::Duration::from_secs(config.timeouts.dap_initialize_secs);
        let request_timeout = std::time::Duration::from_secs(config.timeouts.dap_request_secs);

        // Initialize the adapter with timeout
        tracing::debug!(timeout_secs = init_timeout.as_secs(), "Sending DAP initialize request");
        let capabilities = client.initialize_with_timeout(&adapter_name, init_timeout).await?;
        tracing::debug!(?capabilities, "DAP adapter initialized");

        // Launch the program (DAP: launch must come before initialized event)
        let cwd = std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned());

        // Build launch arguments - adapter-specific fields
        let is_python = adapter_name == "debugpy" 
            || program.extension().map(|e| e == "py").unwrap_or(false);
        let is_go = adapter_name == "go"
            || adapter_name == "delve"
            || adapter_name == "dlv";
        
        let launch_args = LaunchArguments {
            program: program.to_string_lossy().into_owned(),
            args: args.clone(),
            cwd,
            env: None,
            stop_on_entry,
            // lldb-dap specific
            init_commands: None,
            pre_run_commands: None,
            // debugpy specific
            request: if is_python { Some("launch".to_string()) } else { None },
            console: if is_python { Some("internalConsole".to_string()) } else { None },
            python: None, // Let debugpy use its own Python
            just_my_code: if is_python { Some(true) } else { None },
            // Delve (Go) specific - use "exec" for precompiled binaries
            mode: if is_go { Some("exec".to_string()) } else { None },
            // Delve uses stopAtEntry instead of stopOnEntry
            stop_at_entry: if is_go && stop_on_entry { Some(true) } else { None },
            // GDB-based adapters (gdb, cuda-gdb) use stopAtBeginningOfMainSubprogram
            stop_at_beginning_of_main_subprogram: if (adapter_name == "gdb" || adapter_name == "cuda-gdb") && stop_on_entry { Some(true) } else { None },
        };

        tracing::debug!(
            program = %program.display(),
            args = ?args,
            is_python,
            stop_on_entry,
            "Sending DAP launch request"
        );
        
        // For Python/debugpy, we need to use non-blocking launch because debugpy
        // doesn't respond to launch until after configurationDone is sent.
        // We send launch, wait for initialized, send configurationDone, then
        // the launch response arrives.
        if is_python {
            client.launch_no_wait(launch_args).await?;
            tracing::debug!("DAP launch request sent (no-wait mode for Python)");
        } else {
            client.launch(launch_args).await?;
            tracing::debug!("DAP launch request successful");
        }

        // Wait for initialized event (comes after launch per DAP spec)
        tracing::debug!(timeout_secs = request_timeout.as_secs(), "Waiting for DAP initialized event");
        client.wait_initialized_with_timeout(request_timeout).await?;
        tracing::debug!("Received DAP initialized event");

        // Set initial breakpoints before configurationDone
        // This is required for adapters that don't support stopOnEntry (e.g., cdt-gdb-adapter)
        let has_initial_breakpoints = !initial_breakpoints.is_empty();
        if has_initial_breakpoints {
            tracing::debug!(count = initial_breakpoints.len(), "Setting initial breakpoints");

            // Group breakpoints by type (source vs function)
            let mut source_bps: std::collections::HashMap<PathBuf, Vec<dap::SourceBreakpoint>> = std::collections::HashMap::new();
            let mut function_bps: Vec<dap::FunctionBreakpoint> = Vec::new();

            for bp_str in &initial_breakpoints {
                match BreakpointLocation::parse(bp_str) {
                    Ok(BreakpointLocation::Line { file, line }) => {
                        source_bps.entry(file).or_default().push(dap::SourceBreakpoint {
                            line,
                            column: None,
                            condition: None,
                            hit_condition: None,
                            log_message: None,
                        });
                    }
                    Ok(BreakpointLocation::Function { name }) => {
                        function_bps.push(dap::FunctionBreakpoint {
                            name,
                            condition: None,
                            hit_condition: None,
                        });
                    }
                    Err(e) => {
                        tracing::warn!(breakpoint = %bp_str, error = %e, "Failed to parse initial breakpoint");
                    }
                }
            }

            // Set source breakpoints
            for (file, bps) in source_bps {
                match client.set_breakpoints(&file, bps).await {
                    Ok(results) => {
                        for bp in results {
                            tracing::debug!(
                                verified = bp.verified,
                                line = bp.line,
                                "Initial source breakpoint set"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(file = %file.display(), error = %e, "Failed to set initial breakpoints");
                    }
                }
            }

            // Set function breakpoints
            if !function_bps.is_empty() {
                match client.set_function_breakpoints(function_bps).await {
                    Ok(results) => {
                        for bp in results {
                            tracing::debug!(
                                verified = bp.verified,
                                line = bp.line,
                                "Initial function breakpoint set"
                            );
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to set initial function breakpoints");
                    }
                }
            }
        }

        // Signal configuration done - this tells the adapter to start execution
        tracing::debug!("Sending DAP configurationDone request");
        client.configuration_done().await?;
        tracing::debug!("DAP configuration complete, program starting");

        // Take the event receiver (must be done after wait_initialized)
        let events_rx = client
            .take_event_receiver()
            .ok_or_else(|| Error::Internal("Failed to get event receiver".to_string()))?;

        // Initial state: Stopped if stop_on_entry requested, otherwise Running
        // Note: If initial breakpoints are set, the program will stop when it hits them
        let initial_state = if stop_on_entry {
            SessionState::Stopped
        } else {
            SessionState::Running
        };

        Ok(Self {
            client,
            events_rx,
            state: initial_state,
            capabilities,
            program: program.to_path_buf(),
            args,
            adapter_name,
            launched: true,
            source_breakpoints: HashMap::new(),
            function_breakpoints: Vec::new(),
            next_bp_id: 1,
            threads: Vec::new(),
            selected_thread: None,
            stopped_thread: None,
            stopped_reason: None,
            hit_breakpoints: Vec::new(),
            current_frame_index: 0,
            current_frame: None,
            cached_frames: Vec::new(),
            output_buffer: VecDeque::new(),
            max_output_events: config.output.max_events,
            max_output_bytes: config.output.max_bytes_mb * 1024 * 1024,
            current_output_bytes: 0,
            exit_code: None,
            dap_request_timeout: request_timeout,
        })
    }

    /// Create a new debug session by attaching to a process
    pub async fn attach(
        config: &Config,
        pid: u32,
        adapter_name: Option<String>,
    ) -> Result<Self> {
        let adapter_name = adapter_name.unwrap_or_else(|| config.defaults.adapter.clone());

        let adapter_config = config.get_adapter(&adapter_name).ok_or_else(|| {
            Error::adapter_not_found(&adapter_name, &[&adapter_name])
        })?;

        tracing::info!(
            pid,
            adapter = %adapter_name,
            transport = ?adapter_config.transport,
            "Attaching to process"
        );

        let mut client = match adapter_config.transport {
            TransportMode::Stdio => {
                DapClient::spawn(&adapter_config.path, &adapter_config.args).await?
            }
            TransportMode::Tcp => {
                DapClient::spawn_tcp(&adapter_config.path, &adapter_config.args).await?
            }
        };

        // Get configured timeouts
        let init_timeout = std::time::Duration::from_secs(config.timeouts.dap_initialize_secs);
        let request_timeout = std::time::Duration::from_secs(config.timeouts.dap_request_secs);

        let capabilities = client.initialize_with_timeout(&adapter_name, init_timeout).await?;

        // Attach to the process (DAP: attach must come before initialized event)
        client
            .attach(AttachArguments {
                pid,
                wait_for: None,
            })
            .await?;

        // Wait for initialized event (comes after attach per DAP spec)
        client.wait_initialized_with_timeout(request_timeout).await?;

        // Signal configuration done
        client.configuration_done().await?;

        // Take the event receiver (must be done after wait_initialized)
        let events_rx = client
            .take_event_receiver()
            .ok_or_else(|| Error::Internal("Failed to get event receiver".to_string()))?;

        Ok(Self {
            client,
            events_rx,
            state: SessionState::Stopped, // Attached processes start stopped
            capabilities,
            program: PathBuf::from(format!("pid:{}", pid)),
            args: Vec::new(),
            adapter_name,
            launched: false,
            source_breakpoints: HashMap::new(),
            function_breakpoints: Vec::new(),
            next_bp_id: 1,
            threads: Vec::new(),
            selected_thread: None,
            stopped_thread: None,
            stopped_reason: Some("attach".to_string()),
            hit_breakpoints: Vec::new(),
            current_frame_index: 0,
            current_frame: None,
            cached_frames: Vec::new(),
            output_buffer: VecDeque::new(),
            max_output_events: config.output.max_events,
            max_output_bytes: config.output.max_bytes_mb * 1024 * 1024,
            current_output_bytes: 0,
            exit_code: None,
            dap_request_timeout: request_timeout,
        })
    }

    /// Get current state
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Get program path
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// Get adapter name
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Get stopped thread ID
    pub fn stopped_thread(&self) -> Option<i64> {
        self.stopped_thread
    }

    /// Get stopped reason
    pub fn stopped_reason(&self) -> Option<&str> {
        self.stopped_reason.as_deref()
    }

    /// Get exit code if exited
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    /// Process pending events
    pub async fn process_events(&mut self) -> Result<Vec<Event>> {
        let mut events = Vec::new();

        while let Ok(event) = self.events_rx.try_recv() {
            self.handle_event(&event);
            events.push(event);
        }

        Ok(events)
    }

    /// Drain and process any pending events without collecting them
    /// This ensures we don't lose state updates from events while clearing the queue
    fn drain_pending_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            self.handle_event(&event);
        }
    }

    /// Handle a single event
    fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Stopped(body) => {
                self.state = SessionState::Stopped;
                self.stopped_thread = body.thread_id;
                self.stopped_reason = Some(body.reason.clone());
                self.hit_breakpoints = body.hit_breakpoint_ids.clone();
                // Reset frame tracking on stop - user starts at top of stack
                self.current_frame = None;
                self.current_frame_index = 0;
                self.cached_frames.clear();
                tracing::debug!("Stopped: {:?}", body);
            }
            Event::Continued { thread_id, .. } => {
                self.state = SessionState::Running;
                self.stopped_thread = None;
                self.stopped_reason = None;
                self.hit_breakpoints.clear();
                self.current_frame = None;
                self.current_frame_index = 0;
                self.cached_frames.clear();
                tracing::debug!("Continued: thread {}", thread_id);
            }
            Event::Exited(body) => {
                self.state = SessionState::Exited;
                self.exit_code = Some(body.exit_code);
                tracing::info!("Program exited with code {}", body.exit_code);
            }
            Event::Terminated(_) => {
                self.state = SessionState::Exited;
                tracing::info!("Session terminated");
            }
            Event::Output(body) => {
                let category = body.category.clone().unwrap_or_else(|| "console".to_string());
                self.buffer_output(&category, &body.output);
            }
            Event::Thread(body) => {
                tracing::debug!("Thread {}: {}", body.thread_id, body.reason);
                // Update thread list if needed
                if body.reason == "exited" {
                    self.threads.retain(|t| t.id != body.thread_id);
                    // Clear selected thread if it was the one that exited
                    if self.selected_thread == Some(body.thread_id) {
                        self.selected_thread = None;
                    }
                }
            }
            Event::Breakpoint { reason, breakpoint } => {
                tracing::debug!("Breakpoint {}: {:?}", reason, breakpoint);
                // Update breakpoint status if we get change notifications
                if let Some(bp_id) = breakpoint.id {
                    self.update_breakpoint_from_event(bp_id as u32, breakpoint);
                }
            }
            _ => {}
        }
    }

    /// Update breakpoint status from a breakpoint event
    fn update_breakpoint_from_event(&mut self, _id: u32, bp: &dap::Breakpoint) {
        // Try to match by line/source to update verification status
        if let (Some(source), Some(line)) = (&bp.source, bp.line) {
            if let Some(path) = &source.path {
                let path = PathBuf::from(path);
                if let Some(stored_bps) = self.source_breakpoints.get_mut(&path) {
                    for stored in stored_bps.iter_mut() {
                        if let BreakpointLocation::Line { line: stored_line, .. } = &stored.location {
                            if *stored_line == line || stored.actual_line == Some(line) {
                                stored.verified = bp.verified;
                                stored.actual_line = bp.line;
                                stored.message = bp.message.clone();
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Buffer output for later retrieval
    ///
    /// Enforces both max_output_events and max_output_bytes limits.
    /// If a single output message exceeds max_output_bytes, it is truncated.
    fn buffer_output(&mut self, category: &str, output: &str) {
        // Truncate oversized messages to prevent exceeding limits
        let output = if output.len() > self.max_output_bytes {
            tracing::warn!(
                "Output message ({} bytes) exceeds max buffer size ({} bytes), truncating",
                output.len(),
                self.max_output_bytes
            );
            // Truncate to fit, trying to break at a char boundary
            let truncated: String = output.chars().take(self.max_output_bytes).collect();
            truncated
        } else {
            output.to_string()
        };

        let output_bytes = output.len();

        // Enforce byte limit - remove oldest entries until we have space
        while self.current_output_bytes + output_bytes > self.max_output_bytes
            && !self.output_buffer.is_empty()
        {
            if let Some(removed) = self.output_buffer.pop_front() {
                self.current_output_bytes = self.current_output_bytes.saturating_sub(removed.output.len());
            }
        }

        // Enforce event count limit
        while self.output_buffer.len() >= self.max_output_events && !self.output_buffer.is_empty() {
            if let Some(removed) = self.output_buffer.pop_front() {
                self.current_output_bytes = self.current_output_bytes.saturating_sub(removed.output.len());
            }
        }

        // Add the new output
        self.output_buffer.push_back(OutputEvent {
            category: category.to_string(),
            output,
            timestamp: std::time::Instant::now(),
        });
        self.current_output_bytes += output_bytes;
    }

    /// Wait for the program to stop
    ///
    /// This method waits for a stop event (Stopped, Exited, or Terminated) to arrive
    /// through the event channel. Since the background reader task in DapClient
    /// continuously reads events from the adapter, we just need to wait on the channel.
    pub async fn wait_stopped(&mut self, timeout_secs: u64) -> Result<Event> {
        let timeout = std::time::Duration::from_secs(timeout_secs);
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            // Calculate remaining time
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                return Err(Error::AwaitTimeout(timeout_secs));
            }

            // Wait for next event with timeout
            match tokio::time::timeout(remaining, self.events_rx.recv()).await {
                Ok(Some(event)) => {
                    self.handle_event(&event);

                    match &event {
                        Event::Stopped(_) | Event::Exited(_) | Event::Terminated(_) => {
                            return Ok(event);
                        }
                        _ => {
                            // Continue waiting for stop event
                        }
                    }
                }
                Ok(None) => {
                    // Channel closed - adapter crashed or terminated
                    return Err(Error::AdapterCrashed);
                }
                Err(_) => {
                    // Timeout elapsed
                    return Err(Error::AwaitTimeout(timeout_secs));
                }
            }
        }
    }

    /// Add a breakpoint
    pub async fn add_breakpoint(
        &mut self,
        location: BreakpointLocation,
        condition: Option<String>,
        hit_count: Option<u32>,
    ) -> Result<BreakpointInfo> {
        let bp_id = self.next_bp_id;
        self.next_bp_id += 1;

        match &location {
            BreakpointLocation::Line { file, line } => {
                // Add to our tracking
                let stored = StoredBreakpoint {
                    id: bp_id,
                    location: location.clone(),
                    condition: condition.clone(),
                    hit_count,
                    enabled: true,
                    verified: false,
                    actual_line: None,
                    message: None,
                };

                let entry = self.source_breakpoints.entry(file.clone()).or_default();
                entry.push(stored);

                // Send to adapter
                let source_bps = self.collect_source_breakpoints(file);
                let results = self.client.set_breakpoints(file, source_bps).await?;

                // Update verification status
                self.update_source_breakpoint_status(file, &results);

                // Find our breakpoint in results
                let info = self.get_breakpoint_info(bp_id)?;
                Ok(info)
            }
            BreakpointLocation::Function { name } => {
                let stored = StoredBreakpoint {
                    id: bp_id,
                    location: location.clone(),
                    condition: condition.clone(),
                    hit_count,
                    enabled: true,
                    verified: false,
                    actual_line: None,
                    message: None,
                };

                self.function_breakpoints.push(stored);

                // Send all function breakpoints
                let func_bps = self.collect_function_breakpoints();
                let results = self.client.set_function_breakpoints(func_bps).await?;

                // Update verification status
                self.update_function_breakpoint_status(&results);

                let info = self.get_breakpoint_info(bp_id)?;
                Ok(info)
            }
        }
    }

    /// Collect source breakpoints for a file
    fn collect_source_breakpoints(&self, file: &Path) -> Vec<SourceBreakpoint> {
        self.source_breakpoints
            .get(file)
            .map(|bps| {
                bps.iter()
                    .filter(|bp| bp.enabled)
                    .map(|bp| {
                        let line = match &bp.location {
                            BreakpointLocation::Line { line, .. } => *line,
                            _ => 0,
                        };
                        SourceBreakpoint {
                            line,
                            column: None,
                            condition: bp.condition.clone(),
                            hit_condition: bp.hit_count.map(|n| n.to_string()),
                            log_message: None,
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Collect function breakpoints
    fn collect_function_breakpoints(&self) -> Vec<FunctionBreakpoint> {
        self.function_breakpoints
            .iter()
            .filter(|bp| bp.enabled)
            .map(|bp| {
                let name = match &bp.location {
                    BreakpointLocation::Function { name } => name.clone(),
                    _ => String::new(),
                };
                FunctionBreakpoint {
                    name,
                    condition: bp.condition.clone(),
                    hit_condition: bp.hit_count.map(|n| n.to_string()),
                }
            })
            .collect()
    }

    /// Update source breakpoint status from adapter response
    fn update_source_breakpoint_status(&mut self, file: &Path, results: &[Breakpoint]) {
        if let Some(stored) = self.source_breakpoints.get_mut(file) {
            // Match by line number (best effort)
            for (stored_bp, result) in stored.iter_mut().zip(results.iter()) {
                stored_bp.verified = result.verified;
                stored_bp.actual_line = result.line;
                stored_bp.message = result.message.clone();
            }
        }
    }

    /// Update function breakpoint status from adapter response
    fn update_function_breakpoint_status(&mut self, results: &[Breakpoint]) {
        for (stored_bp, result) in self.function_breakpoints.iter_mut().zip(results.iter()) {
            stored_bp.verified = result.verified;
            stored_bp.actual_line = result.line;
            stored_bp.message = result.message.clone();
        }
    }

    /// Get breakpoint info by ID
    fn get_breakpoint_info(&self, id: u32) -> Result<BreakpointInfo> {
        // Search source breakpoints
        for (file, bps) in &self.source_breakpoints {
            if let Some(bp) = bps.iter().find(|bp| bp.id == id) {
                return Ok(BreakpointInfo {
                    id: bp.id,
                    verified: bp.verified,
                    source: Some(file.to_string_lossy().into_owned()),
                    line: bp.actual_line.or(match &bp.location {
                        BreakpointLocation::Line { line, .. } => Some(*line),
                        _ => None,
                    }),
                    message: bp.message.clone(),
                    enabled: bp.enabled,
                    condition: bp.condition.clone(),
                    hit_count: bp.hit_count,
                });
            }
        }

        // Search function breakpoints
        if let Some(bp) = self.function_breakpoints.iter().find(|bp| bp.id == id) {
            return Ok(BreakpointInfo {
                id: bp.id,
                verified: bp.verified,
                source: match &bp.location {
                    BreakpointLocation::Function { name } => Some(name.clone()),
                    _ => None,
                },
                line: bp.actual_line,
                message: bp.message.clone(),
                enabled: bp.enabled,
                condition: bp.condition.clone(),
                hit_count: bp.hit_count,
            });
        }

        Err(Error::BreakpointNotFound { id })
    }

    /// Remove a breakpoint by ID
    pub async fn remove_breakpoint(&mut self, id: u32) -> Result<()> {
        // Find and remove from source breakpoints
        let mut file_to_update = None;
        for (file, bps) in &mut self.source_breakpoints {
            if let Some(pos) = bps.iter().position(|bp| bp.id == id) {
                bps.remove(pos);
                file_to_update = Some(file.clone());
                break;
            }
        }

        if let Some(file) = file_to_update {
            let source_bps = self.collect_source_breakpoints(&file);
            self.client.set_breakpoints(&file, source_bps).await?;
            return Ok(());
        }

        // Try function breakpoints
        if let Some(pos) = self.function_breakpoints.iter().position(|bp| bp.id == id) {
            self.function_breakpoints.remove(pos);
            let func_bps = self.collect_function_breakpoints();
            self.client.set_function_breakpoints(func_bps).await?;
            return Ok(());
        }

        Err(Error::BreakpointNotFound { id })
    }

    /// Remove all breakpoints
    pub async fn remove_all_breakpoints(&mut self) -> Result<()> {
        // Clear source breakpoints
        let files: Vec<_> = self.source_breakpoints.keys().cloned().collect();
        for file in files {
            self.client.set_breakpoints(&file, vec![]).await?;
        }
        self.source_breakpoints.clear();

        // Clear function breakpoints
        self.client.set_function_breakpoints(vec![]).await?;
        self.function_breakpoints.clear();

        Ok(())
    }

    /// List all breakpoints
    pub fn list_breakpoints(&self) -> Vec<BreakpointInfo> {
        let mut result = Vec::new();

        for (file, bps) in &self.source_breakpoints {
            for bp in bps {
                result.push(BreakpointInfo {
                    id: bp.id,
                    verified: bp.verified,
                    source: Some(file.to_string_lossy().into_owned()),
                    line: bp.actual_line.or(match &bp.location {
                        BreakpointLocation::Line { line, .. } => Some(*line),
                        _ => None,
                    }),
                    message: bp.message.clone(),
                    enabled: bp.enabled,
                    condition: bp.condition.clone(),
                    hit_count: bp.hit_count,
                });
            }
        }

        for bp in &self.function_breakpoints {
            result.push(BreakpointInfo {
                id: bp.id,
                verified: bp.verified,
                source: match &bp.location {
                    BreakpointLocation::Function { name } => Some(name.clone()),
                    _ => None,
                },
                line: bp.actual_line,
                message: bp.message.clone(),
                enabled: bp.enabled,
                condition: bp.condition.clone(),
                hit_count: bp.hit_count,
            });
        }

        result
    }

    /// Continue execution
    pub async fn continue_execution(&mut self) -> Result<()> {
        self.ensure_stopped()?;

        // Process any pending events before sending continue request
        // This ensures we don't lose state updates while clearing the queue
        self.drain_pending_events();

        let thread_id = self.get_thread_id().await?;
        self.client.continue_execution(thread_id).await?;
        self.state = SessionState::Running;
        self.stopped_thread = None;
        self.stopped_reason = None;

        Ok(())
    }

    /// Step over (next)
    pub async fn next(&mut self) -> Result<()> {
        self.ensure_stopped()?;

        // Process any pending events before sending step request
        self.drain_pending_events();

        let thread_id = self.get_thread_id().await?;
        self.client.next(thread_id).await?;
        self.state = SessionState::Running;

        Ok(())
    }

    /// Step into
    pub async fn step_in(&mut self) -> Result<()> {
        self.ensure_stopped()?;

        // Process any pending events before sending step request
        self.drain_pending_events();

        let thread_id = self.get_thread_id().await?;
        self.client.step_in(thread_id).await?;
        self.state = SessionState::Running;

        Ok(())
    }

    /// Step out
    pub async fn step_out(&mut self) -> Result<()> {
        self.ensure_stopped()?;

        // Process any pending events before sending step request
        self.drain_pending_events();

        let thread_id = self.get_thread_id().await?;
        self.client.step_out(thread_id).await?;
        self.state = SessionState::Running;

        Ok(())
    }

    /// Pause execution
    pub async fn pause(&mut self) -> Result<()> {
        if self.state != SessionState::Running {
            return Err(Error::invalid_state("pause", &self.state.to_string()));
        }

        let thread_id = self.get_thread_id().await?;
        self.client.pause(thread_id).await?;

        Ok(())
    }

    /// Get stack trace
    pub async fn stack_trace(&mut self, limit: usize) -> Result<Vec<StackFrame>> {
        self.ensure_stopped()?;

        let thread_id = self.get_thread_id().await?;
        let frames = self.client.stack_trace(thread_id, limit as i64).await?;

        // Cache the top frame ID
        if let Some(frame) = frames.first() {
            self.current_frame = Some(frame.id);
        }

        Ok(frames)
    }

    /// Get threads
    pub async fn get_threads(&mut self) -> Result<Vec<Thread>> {
        self.threads = self.client.threads().await?;
        Ok(self.threads.clone())
    }

    /// Get scopes for current frame
    pub async fn get_scopes(&mut self, frame_id: Option<i64>) -> Result<Vec<Scope>> {
        self.ensure_stopped()?;

        // Auto-fetch top frame if no frame specified and current_frame is not set
        let frame_id = match frame_id.or(self.current_frame) {
            Some(id) => id,
            None => {
                // Fetch stack trace to get the top frame
                let thread_id = self.get_thread_id().await?;
                let frames = self.client.stack_trace(thread_id, 1).await?;
                let frame = frames.first().ok_or_else(|| {
                    Error::Internal("No stack frames available".to_string())
                })?;
                self.current_frame = Some(frame.id);
                frame.id
            }
        };

        self.client.scopes(frame_id).await
    }

    /// Get variables
    pub async fn get_variables(&mut self, reference: i64) -> Result<Vec<Variable>> {
        self.ensure_stopped()?;
        self.client.variables(reference).await
    }

    /// Get local variables for current frame
    pub async fn get_locals(&mut self, frame_id: Option<i64>) -> Result<Vec<Variable>> {
        let scopes = self.get_scopes(frame_id).await?;

        // Find the "Locals" scope
        let locals_scope = scopes.iter().find(|s| s.name == "Locals" || s.name == "Local");

        if let Some(scope) = locals_scope {
            self.get_variables(scope.variables_reference).await
        } else if let Some(scope) = scopes.first() {
            // Fall back to first scope
            self.get_variables(scope.variables_reference).await
        } else {
            Ok(Vec::new())
        }
    }

    /// Evaluate an expression
    pub async fn evaluate(
        &mut self,
        expression: &str,
        frame_id: Option<i64>,
        context: &str,
    ) -> Result<dap::EvaluateResponseBody> {
        self.ensure_stopped()?;

        // Auto-fetch top frame if no frame specified and current_frame is not set
        let frame_id = match frame_id.or(self.current_frame) {
            Some(id) => Some(id),
            None => {
                // Fetch stack trace to get the top frame
                let thread_id = self.get_thread_id().await?;
                let frames = self.client.stack_trace(thread_id, 1).await?;
                if let Some(frame) = frames.first() {
                    self.current_frame = Some(frame.id);
                    Some(frame.id)
                } else {
                    None
                }
            }
        };
        self.client.evaluate(expression, frame_id, context).await
    }

    /// Get buffered output
    pub fn get_output(&mut self, tail: Option<usize>, clear: bool) -> Vec<OutputEvent> {
        let result: Vec<OutputEvent> = if let Some(n) = tail {
            self.output_buffer.iter().rev().take(n).cloned().rev().collect()
        } else {
            self.output_buffer.iter().cloned().collect()
        };

        if clear {
            self.output_buffer.clear();
        }

        result
    }

    /// Detach from the debuggee (keep it running)
    pub async fn detach(&mut self) -> Result<()> {
        self.state = SessionState::Terminating;
        self.client.disconnect(false).await?;
        Ok(())
    }

    /// Stop the debuggee and terminate session
    pub async fn stop(&mut self) -> Result<()> {
        self.state = SessionState::Terminating;
        self.client.disconnect(self.launched).await?;
        self.client.terminate().await?;
        Ok(())
    }

    /// Restart the debug session using the DAP restart request.
    ///
    /// Note: The caller (handler) should check `supports_restart_request` capability
    /// before calling this method. If the adapter doesn't support restart, the
    /// user should be instructed to use 'debugger stop' then 'debugger start'.
    pub async fn restart(&mut self) -> Result<()> {
        self.client.restart(false).await?;
        self.state = SessionState::Running;
        // Clear frame/stop state since we're restarting
        self.stopped_thread = None;
        self.stopped_reason = None;
        self.current_frame = None;
        self.current_frame_index = 0;
        self.cached_frames.clear();
        Ok(())
    }

    /// Select a thread for debugging operations
    ///
    /// Returns an error if the thread is not found in the current thread list.
    /// Note: The thread list may be stale; call `get_threads()` first to refresh.
    pub fn select_thread(&mut self, thread_id: i64) -> Result<()> {
        // Verify thread exists in our known thread list
        if !self.threads.iter().any(|t| t.id == thread_id) {
            return Err(Error::Internal(format!(
                "Thread {} not found. Use 'threads' command to see available threads.",
                thread_id
            )));
        }

        self.selected_thread = Some(thread_id);
        // Reset frame selection when switching threads
        self.current_frame_index = 0;
        self.current_frame = None;
        self.cached_frames.clear();

        Ok(())
    }

    /// Get the currently selected thread (for UI display)
    pub fn get_selected_thread(&self) -> Option<i64> {
        self.selected_thread.or(self.stopped_thread)
    }

    /// Select a stack frame by index (0 = top/innermost)
    pub async fn select_frame(&mut self, frame_index: usize) -> Result<StackFrame> {
        self.ensure_stopped()?;

        // Fetch frames if not cached or if requesting beyond cache
        if self.cached_frames.is_empty() || frame_index >= self.cached_frames.len() {
            let thread_id = self.get_thread_id().await?;
            // Fetch enough frames to include the requested one
            let needed = (frame_index + 1).max(20);
            self.cached_frames = self.client.stack_trace(thread_id, needed as i64).await?;
        }

        if frame_index >= self.cached_frames.len() {
            return Err(Error::FrameNotFound(frame_index));
        }

        self.current_frame_index = frame_index;
        self.current_frame = Some(self.cached_frames[frame_index].id);

        Ok(self.cached_frames[frame_index].clone())
    }

    /// Move up the stack (to caller frame)
    pub async fn frame_up(&mut self) -> Result<StackFrame> {
        let new_index = self.current_frame_index + 1;
        self.select_frame(new_index).await
    }

    /// Move down the stack (toward innermost/current frame)
    pub async fn frame_down(&mut self) -> Result<StackFrame> {
        if self.current_frame_index == 0 {
            return Err(Error::invalid_state("frame down", "already at innermost frame"));
        }
        let new_index = self.current_frame_index - 1;
        self.select_frame(new_index).await
    }

    /// Get current frame index
    pub fn get_current_frame_index(&self) -> usize {
        self.current_frame_index
    }

    /// Enable a breakpoint
    pub async fn enable_breakpoint(&mut self, id: u32) -> Result<()> {
        self.set_breakpoint_enabled(id, true).await
    }

    /// Disable a breakpoint
    pub async fn disable_breakpoint(&mut self, id: u32) -> Result<()> {
        self.set_breakpoint_enabled(id, false).await
    }

    /// Set breakpoint enabled state
    async fn set_breakpoint_enabled(&mut self, id: u32, enabled: bool) -> Result<()> {
        // Find and update the breakpoint
        let mut file_to_update = None;
        let mut is_function_bp = false;

        for (file, bps) in &mut self.source_breakpoints {
            if let Some(bp) = bps.iter_mut().find(|bp| bp.id == id) {
                bp.enabled = enabled;
                file_to_update = Some(file.clone());
                break;
            }
        }

        if file_to_update.is_none() {
            if let Some(bp) = self.function_breakpoints.iter_mut().find(|bp| bp.id == id) {
                bp.enabled = enabled;
                is_function_bp = true;
            } else {
                return Err(Error::BreakpointNotFound { id });
            }
        }

        // Re-send breakpoints to adapter
        if let Some(file) = file_to_update {
            let source_bps = self.collect_source_breakpoints(&file);
            let results = self.client.set_breakpoints(&file, source_bps).await?;
            self.update_source_breakpoint_status(&file, &results);
        } else if is_function_bp {
            let func_bps = self.collect_function_breakpoints();
            let results = self.client.set_function_breakpoints(func_bps).await?;
            self.update_function_breakpoint_status(&results);
        }

        Ok(())
    }

    /// Get adapter capabilities
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    /// Check if adapter supports a capability before using it
    pub fn supports_function_breakpoints(&self) -> bool {
        self.capabilities.supports_function_breakpoints
    }

    /// Check if adapter supports conditional breakpoints
    pub fn supports_conditional_breakpoints(&self) -> bool {
        self.capabilities.supports_conditional_breakpoints
    }

    /// Check if adapter supports hit conditional breakpoints
    pub fn supports_hit_conditional_breakpoints(&self) -> bool {
        self.capabilities.supports_hit_conditional_breakpoints
    }

    /// Ensure we're in stopped state for inspection commands
    fn ensure_stopped(&self) -> Result<()> {
        match self.state {
            SessionState::Stopped => Ok(()),
            SessionState::Exited => {
                Err(Error::ProgramExited(self.exit_code.unwrap_or(0)))
            }
            _ => Err(Error::invalid_state("inspect", &self.state.to_string())),
        }
    }

    /// Get a thread ID (preferring selected > stopped > first)
    async fn get_thread_id(&mut self) -> Result<i64> {
        // Prefer explicitly selected thread
        if let Some(id) = self.selected_thread {
            return Ok(id);
        }

        // Fall back to stopped thread
        if let Some(id) = self.stopped_thread {
            return Ok(id);
        }

        // Fetch threads and use the first one
        if self.threads.is_empty() {
            self.threads = self.client.threads().await?;
        }

        self.threads
            .first()
            .map(|t| t.id)
            .ok_or_else(|| Error::Internal("No threads available".to_string()))
    }
}
