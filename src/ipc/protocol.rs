//! IPC protocol message types
//!
//! Defines the request/response format for CLI ↔ daemon communication.
//! Uses a simple length-prefixed JSON protocol.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::common::error::IpcError;

/// IPC request from CLI to daemon
#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    /// Request ID for matching responses
    pub id: u64,
    /// The command to execute
    pub command: Command,
}

/// IPC response from daemon to CLI
#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    /// Request ID this response corresponds to
    pub id: u64,
    /// Whether the command succeeded
    pub success: bool,
    /// Result data on success
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error information on failure
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

impl Response {
    /// Create a success response
    pub fn success(id: u64, result: serde_json::Value) -> Self {
        Self {
            id,
            success: true,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response
    pub fn error(id: u64, error: IpcError) -> Self {
        Self {
            id,
            success: false,
            result: None,
            error: Some(error),
        }
    }

    /// Create a success response with no data
    pub fn ok(id: u64) -> Self {
        Self {
            id,
            success: true,
            result: Some(serde_json::json!({})),
            error: None,
        }
    }
}

/// Commands that can be sent from CLI to daemon
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    // === Session Management ===
    /// Start debugging a program
    Start {
        program: PathBuf,
        args: Vec<String>,
        adapter: Option<String>,
        stop_on_entry: bool,
    },

    /// Attach to a running process
    Attach {
        pid: u32,
        adapter: Option<String>,
    },

    /// Detach from process (keeps it running)
    Detach,

    /// Stop debugging (terminates debuggee)
    Stop,

    /// Restart program with same arguments
    Restart,

    /// Get session status
    Status,

    // === Breakpoints ===
    /// Add a breakpoint
    BreakpointAdd {
        location: BreakpointLocation,
        condition: Option<String>,
        hit_count: Option<u32>,
    },

    /// Remove a breakpoint
    BreakpointRemove {
        id: Option<u32>,
        all: bool,
    },

    /// List all breakpoints
    BreakpointList,

    /// Enable a breakpoint
    BreakpointEnable { id: u32 },

    /// Disable a breakpoint
    BreakpointDisable { id: u32 },

    // === Execution Control ===
    /// Continue execution
    Continue,

    /// Step over (next line, skip function calls)
    Next,

    /// Step into (next line, enter function calls)
    StepIn,

    /// Step out (run until function returns)
    StepOut,

    /// Pause execution
    Pause,

    // === State Inspection ===
    /// Get stack trace
    StackTrace {
        thread_id: Option<i64>,
        limit: usize,
    },

    /// Get local variables
    Locals { frame_id: Option<i64> },

    /// Evaluate expression
    Evaluate {
        expression: String,
        frame_id: Option<i64>,
        context: EvaluateContext,
    },

    /// Get scopes for a frame
    Scopes { frame_id: i64 },

    /// Get variables in a scope
    Variables { reference: i64 },

    // === Thread/Frame Management ===
    /// List all threads
    Threads,

    /// Switch to thread
    ThreadSelect { id: i64 },

    /// Select stack frame
    FrameSelect { number: usize },

    // === Context ===
    /// Get current position with source context
    Context { lines: usize },

    // === Async ===
    /// Wait for next stop event
    Await { timeout_secs: u64 },

    // === Output ===
    /// Get buffered output
    GetOutput {
        tail: Option<usize>,
        clear: bool,
    },

    /// Subscribe to output events (for --follow)
    SubscribeOutput,

    // === Shutdown ===
    /// Shutdown the daemon
    Shutdown,
}

/// Breakpoint location specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BreakpointLocation {
    /// File and line number
    Line { file: PathBuf, line: u32 },
    /// Function name
    Function { name: String },
}

impl BreakpointLocation {
    /// Parse a location string like "file.rs:42" or "main"
    pub fn parse(s: &str) -> Result<Self, crate::common::Error> {
        // Handle file:line format, careful with Windows paths like "C:\path\file.rs:10"
        // Strategy: find the last ':' that's followed by digits only
        if let Some(colon_idx) = s.rfind(':') {
            let (file_part, line_part) = s.split_at(colon_idx);
            let line_str = &line_part[1..]; // Skip the ':'

            // Only treat as file:line if the part after ':' is a valid line number
            if !line_str.is_empty() && line_str.chars().all(|c| c.is_ascii_digit()) {
                let line: u32 = line_str.parse().map_err(|_| {
                    crate::common::Error::InvalidLocation(format!(
                        "invalid line number: {}",
                        line_str
                    ))
                })?;
                return Ok(Self::Line {
                    file: PathBuf::from(file_part),
                    line,
                });
            }
        }

        // No valid file:line pattern, treat as function name
        Ok(Self::Function {
            name: s.to_string(),
        })
    }
}

impl std::fmt::Display for BreakpointLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Line { file, line } => write!(f, "{}:{}", file.display(), line),
            Self::Function { name } => write!(f, "{}", name),
        }
    }
}

/// Context for expression evaluation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EvaluateContext {
    /// Watch expression (read-only evaluation)
    #[default]
    Watch,
    /// REPL evaluation (can have side effects)
    Repl,
    /// Hover evaluation
    Hover,
}

// === Result types for responses ===

/// Status response
#[derive(Debug, Serialize, Deserialize)]
pub struct StatusResult {
    pub daemon_running: bool,
    pub session_active: bool,
    pub state: Option<String>,
    pub program: Option<String>,
    pub adapter: Option<String>,
    pub stopped_thread: Option<i64>,
    pub stopped_reason: Option<String>,
}

/// Breakpoint information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakpointInfo {
    pub id: u32,
    pub verified: bool,
    pub source: Option<String>,
    pub line: Option<u32>,
    pub message: Option<String>,
    pub enabled: bool,
    pub condition: Option<String>,
    pub hit_count: Option<u32>,
}

/// Stack frame information
#[derive(Debug, Serialize, Deserialize)]
pub struct StackFrameInfo {
    pub id: i64,
    pub name: String,
    pub source: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Thread information
#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub id: i64,
    pub name: String,
    pub state: Option<String>,
}

/// Variable information
#[derive(Debug, Serialize, Deserialize)]
pub struct VariableInfo {
    pub name: String,
    pub value: String,
    pub type_name: Option<String>,
    pub variables_reference: i64,
}

/// Stop event result
#[derive(Debug, Serialize, Deserialize)]
pub struct StopResult {
    pub reason: String,
    pub description: Option<String>,
    pub thread_id: i64,
    pub all_threads_stopped: bool,
    pub hit_breakpoint_ids: Vec<u32>,
    /// Current location info
    pub source: Option<String>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Evaluate result
#[derive(Debug, Serialize, Deserialize)]
pub struct EvaluateResult {
    pub result: String,
    pub type_name: Option<String>,
    pub variables_reference: i64,
}

/// Context result with source code
#[derive(Debug, Serialize, Deserialize)]
pub struct ContextResult {
    pub thread_id: i64,
    pub source: Option<String>,
    pub line: u32,
    pub column: Option<u32>,
    pub function: Option<String>,
    /// Source lines with line numbers
    pub source_lines: Vec<SourceLine>,
    /// Local variables
    pub locals: Vec<VariableInfo>,
}

/// A source line with its number
#[derive(Debug, Serialize, Deserialize)]
pub struct SourceLine {
    pub number: u32,
    pub content: String,
    pub is_current: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_line() {
        let loc = BreakpointLocation::parse("src/main.rs:42").unwrap();
        match loc {
            BreakpointLocation::Line { file, line } => {
                assert_eq!(file, PathBuf::from("src/main.rs"));
                assert_eq!(line, 42);
            }
            _ => panic!("Expected Line variant"),
        }
    }

    #[test]
    fn test_parse_function() {
        let loc = BreakpointLocation::parse("main").unwrap();
        match loc {
            BreakpointLocation::Function { name } => {
                assert_eq!(name, "main");
            }
            _ => panic!("Expected Function variant"),
        }
    }

    #[test]
    fn test_parse_namespaced_function() {
        let loc = BreakpointLocation::parse("mymod::MyStruct::method").unwrap();
        match loc {
            BreakpointLocation::Function { name } => {
                assert_eq!(name, "mymod::MyStruct::method");
            }
            _ => panic!("Expected Function variant"),
        }
    }

    #[cfg(windows)]
    #[test]
    fn test_parse_windows_path() {
        let loc = BreakpointLocation::parse(r"C:\Users\test\src\main.rs:42").unwrap();
        match loc {
            BreakpointLocation::Line { file, line } => {
                assert_eq!(file, PathBuf::from(r"C:\Users\test\src\main.rs"));
                assert_eq!(line, 42);
            }
            _ => panic!("Expected Line variant"),
        }
    }
}
