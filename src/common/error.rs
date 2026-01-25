//! Error types for the debugger CLI
//!
//! Error messages are designed to be clear and actionable for LLM agents,
//! with hints on how to resolve common issues.

use std::io;
use thiserror::Error;

/// Result type alias using our Error type
pub type Result<T> = std::result::Result<T, Error>;

/// Main error type for the debugger CLI
#[derive(Error, Debug)]
pub enum Error {
    // === Daemon/Connection Errors ===
    #[error("Daemon not running. Start a session with 'debugger start <program>'")]
    DaemonNotRunning,

    #[error("Failed to spawn daemon: timed out waiting for socket after {0} seconds")]
    DaemonSpawnTimeout(u64),

    #[error("Failed to connect to daemon: {0}")]
    DaemonConnectionFailed(#[source] io::Error),

    #[error("Daemon communication error: {0}")]
    DaemonCommunication(String),

    // === Session Errors ===
    #[error("No debug session active. Use 'debugger start <program>' or 'debugger attach <pid>' first")]
    SessionNotActive,

    #[error("Debug session already active. Use 'debugger stop' first to end current session")]
    SessionAlreadyActive,

    #[error("Session terminated unexpectedly: {0}")]
    SessionTerminated(String),

    #[error("Program has exited with code {0}")]
    ProgramExited(i32),

    // === Adapter Errors ===
    #[error("Debug adapter '{name}' not found. Searched: {searched}")]
    AdapterNotFound { name: String, searched: String },

    #[error("Debug adapter failed to start: {0}")]
    AdapterStartFailed(String),

    #[error("Debug adapter crashed unexpectedly")]
    AdapterCrashed,

    #[error("Debug adapter returned error: {0}")]
    AdapterError(String),

    // === DAP Protocol Errors ===
    #[error("DAP protocol error: {0}")]
    DapProtocol(String),

    #[error("DAP request '{command}' failed: {message}")]
    DapRequestFailed { command: String, message: String },

    #[error("DAP initialization failed: {0}")]
    DapInitFailed(String),

    // === Breakpoint Errors ===
    #[error("Invalid breakpoint location: {0}")]
    InvalidLocation(String),

    #[error("Breakpoint {id} not found")]
    BreakpointNotFound { id: u32 },

    #[error("Failed to set breakpoint at {location}: {reason}")]
    BreakpointFailed { location: String, reason: String },

    // === Execution Errors ===
    #[error("Cannot {action} while program is {state}")]
    InvalidState { action: String, state: String },

    #[error("Thread {0} not found")]
    ThreadNotFound(i64),

    #[error("Frame {0} not found")]
    FrameNotFound(usize),

    // === Timeout Errors ===
    #[error("Operation timed out after {0} seconds")]
    Timeout(u64),

    #[error("Await timed out after {0} seconds. Program may still be running - use 'debugger status' to check")]
    AwaitTimeout(u64),

    // === Configuration Errors ===
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Invalid configuration file: {0}")]
    ConfigParse(String),

    // === IO Errors ===
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Failed to read file '{path}': {error}")]
    FileRead { path: String, error: String },

    // === Serialization Errors ===
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // === Test Errors ===
    #[error("Test assertion failed: {0}")]
    TestAssertion(String),

    // === Internal Errors ===
    #[error("Internal error: {0}")]
    Internal(String),
}

impl Error {
    /// Create an adapter not found error with search paths
    pub fn adapter_not_found<S: AsRef<str>>(name: &str, paths: &[S]) -> Self {
        Self::AdapterNotFound {
            name: name.to_string(),
            searched: paths.iter().map(|s| s.as_ref()).collect::<Vec<_>>().join(", "),
        }
    }

    /// Create a DAP request failed error
    pub fn dap_request_failed(command: &str, message: &str) -> Self {
        Self::DapRequestFailed {
            command: command.to_string(),
            message: message.to_string(),
        }
    }

    /// Create an invalid state error
    pub fn invalid_state(action: &str, state: &str) -> Self {
        Self::InvalidState {
            action: action.to_string(),
            state: state.to_string(),
        }
    }

    /// Create a breakpoint failed error
    pub fn breakpoint_failed(location: &str, reason: &str) -> Self {
        Self::BreakpointFailed {
            location: location.to_string(),
            reason: reason.to_string(),
        }
    }
}

/// IPC-serializable error for daemon responses
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

impl From<&Error> for IpcError {
    fn from(e: &Error) -> Self {
        let code = match e {
            Error::DaemonNotRunning => "DAEMON_NOT_RUNNING",
            Error::SessionNotActive => "SESSION_NOT_ACTIVE",
            Error::SessionAlreadyActive => "SESSION_ALREADY_ACTIVE",
            Error::AdapterNotFound { .. } => "ADAPTER_NOT_FOUND",
            Error::InvalidLocation(_) => "INVALID_LOCATION",
            Error::BreakpointNotFound { .. } => "BREAKPOINT_NOT_FOUND",
            Error::InvalidState { .. } => "INVALID_STATE",
            Error::ThreadNotFound(_) => "THREAD_NOT_FOUND",
            Error::FrameNotFound(_) => "FRAME_NOT_FOUND",
            Error::Timeout(_) | Error::AwaitTimeout(_) => "TIMEOUT",
            Error::ProgramExited(_) => "PROGRAM_EXITED",
            Error::DapRequestFailed { .. } => "DAP_REQUEST_FAILED",
            _ => "INTERNAL_ERROR",
        }
        .to_string();

        Self {
            code,
            message: e.to_string(),
        }
    }
}

impl From<IpcError> for Error {
    fn from(e: IpcError) -> Self {
        // Map IPC errors back to our error types where possible
        match e.code.as_str() {
            "SESSION_NOT_ACTIVE" => Error::SessionNotActive,
            "SESSION_ALREADY_ACTIVE" => Error::SessionAlreadyActive,
            "TIMEOUT" => Error::Timeout(0),
            _ => Error::DaemonCommunication(e.message),
        }
    }
}
