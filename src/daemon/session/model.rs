//! Reducer-owned session state.
//!
//! `SessionModel` holds everything derived from DAP events and command
//! side-effects: lifecycle state, thread/frame selection, stop details,
//! breakpoint bookkeeping, and exit status. It contains no I/O; `DebugSession`
//! orchestrates the adapter and feeds events into `apply`.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::dap::{self, Event, StackFrame, StoppedEventBody, Thread};
use crate::ipc::protocol::{BreakpointInfo, BreakpointLocation};

/// Debug session state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
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
            Self::Running => write!(f, "running"),
            Self::Stopped => write!(f, "stopped"),
            Self::Exited => write!(f, "exited"),
            Self::Terminating => write!(f, "terminating"),
        }
    }
}

/// Stored breakpoint information
#[derive(Debug, Clone)]
pub(super) struct StoredBreakpoint {
    pub(super) id: u32,
    pub(super) location: BreakpointLocation,
    pub(super) condition: Option<String>,
    pub(super) hit_count: Option<u32>,
    pub(super) enabled: bool,
    pub(super) verified: bool,
    pub(super) actual_line: Option<u32>,
    pub(super) message: Option<String>,
}

impl StoredBreakpoint {
    pub(super) fn info(&self, source: Option<String>) -> BreakpointInfo {
        BreakpointInfo {
            id: self.id,
            verified: self.verified,
            source,
            line: self.actual_line.or(match &self.location {
                BreakpointLocation::Line { line, .. } => Some(*line),
                _ => None,
            }),
            message: self.message.clone(),
            enabled: self.enabled,
            condition: self.condition.clone(),
            hit_count: self.hit_count,
        }
    }
}

/// State container reduced from DAP events and command side-effects.
#[derive(Debug)]
pub(super) struct SessionModel {
    /// Current session state
    pub(super) state: SessionState,
    /// All breakpoints by source file
    pub(super) source_breakpoints: HashMap<PathBuf, Vec<StoredBreakpoint>>,
    /// Function breakpoints
    pub(super) function_breakpoints: Vec<StoredBreakpoint>,
    /// Next breakpoint ID
    pub(super) next_bp_id: u32,
    /// Cached threads
    pub(super) threads: Vec<Thread>,
    /// Currently selected thread (may differ from stopped thread)
    pub(super) selected_thread: Option<i64>,
    /// Currently stopped thread
    pub(super) stopped_thread: Option<i64>,
    /// Reason for last stop
    pub(super) stopped_reason: Option<String>,
    /// Full body of the last stopped event, cleared when execution resumes
    pub(super) last_stop: Option<StoppedEventBody>,
    /// Hit breakpoint IDs from last stop
    pub(super) hit_breakpoints: Vec<u32>,
    /// Current frame index (0 = top of stack)
    pub(super) current_frame_index: usize,
    /// Current frame ID (for variable inspection)
    pub(super) current_frame: Option<i64>,
    /// Cached stack frames for current stop
    pub(super) cached_frames: Vec<StackFrame>,
    /// Exit code if program exited
    pub(super) exit_code: Option<i32>,
}

impl SessionModel {
    pub(super) fn new(initial_state: SessionState) -> Self {
        Self {
            state: initial_state,
            source_breakpoints: HashMap::new(),
            function_breakpoints: Vec::new(),
            next_bp_id: 1,
            threads: Vec::new(),
            selected_thread: None,
            stopped_thread: None,
            stopped_reason: None,
            last_stop: None,
            hit_breakpoints: Vec::new(),
            current_frame_index: 0,
            current_frame: None,
            cached_frames: Vec::new(),
            exit_code: None,
        }
    }

    /// Reduce a single DAP event into the model. Output events are not state
    /// and are routed to the output buffer by `DebugSession` instead.
    pub(super) fn apply(&mut self, event: &Event) {
        match event {
            Event::Stopped(body) => {
                self.state = SessionState::Stopped;
                self.stopped_thread = body.thread_id;
                self.selected_thread = body.thread_id;
                self.stopped_reason = Some(body.reason.clone());
                self.last_stop = Some(body.clone());
                self.hit_breakpoints = body.hit_breakpoint_ids.clone();
                // Reset frame tracking on stop - user starts at top of stack
                self.reset_frame_selection();
                tracing::debug!("Stopped: {:?}", body);
            }
            Event::Continued { thread_id, .. } => {
                self.on_resumed();
                tracing::debug!("Continued: thread {}", thread_id);
            }
            Event::Exited(body) => {
                self.state = SessionState::Exited;
                self.selected_thread = None;
                self.exit_code = Some(body.exit_code);
                tracing::info!("Program exited with code {}", body.exit_code);
            }
            Event::Terminated(_) => {
                self.state = SessionState::Exited;
                self.selected_thread = None;
                tracing::info!("Session terminated");
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
                    self.update_breakpoint_from_event(bp_id, breakpoint);
                }
            }
            _ => {}
        }
    }

    /// Clear stop-derived state when execution resumes (continue/step).
    pub(super) fn on_resumed(&mut self) {
        self.state = SessionState::Running;
        self.selected_thread = None;
        self.stopped_thread = None;
        self.stopped_reason = None;
        self.last_stop = None;
        self.hit_breakpoints.clear();
        self.reset_frame_selection();
    }

    pub(super) fn reset_frame_selection(&mut self) {
        self.current_frame = None;
        self.current_frame_index = 0;
        self.cached_frames.clear();
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

    /// Get breakpoint info by ID
    pub(super) fn breakpoint_info(&self, id: u32) -> Option<BreakpointInfo> {
        for (file, bps) in &self.source_breakpoints {
            if let Some(bp) = bps.iter().find(|bp| bp.id == id) {
                return Some(bp.info(Some(file.to_string_lossy().into_owned())));
            }
        }

        self.function_breakpoints
            .iter()
            .find(|bp| bp.id == id)
            .map(|bp| {
                bp.info(match &bp.location {
                    BreakpointLocation::Function { name } => Some(name.clone()),
                    _ => None,
                })
            })
    }

    /// List all breakpoints
    pub(super) fn list_breakpoints(&self) -> Vec<BreakpointInfo> {
        let mut result = Vec::new();

        for (file, bps) in &self.source_breakpoints {
            for bp in bps {
                result.push(bp.info(Some(file.to_string_lossy().into_owned())));
            }
        }

        for bp in &self.function_breakpoints {
            result.push(bp.info(match &bp.location {
                BreakpointLocation::Function { name } => Some(name.clone()),
                _ => None,
            }));
        }

        result
    }
}
