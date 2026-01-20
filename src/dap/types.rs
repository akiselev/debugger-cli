//! DAP message types
//!
//! These types represent the Debug Adapter Protocol messages.
//! See: https://microsoft.github.io/debug-adapter-protocol/specification

use serde::{Deserialize, Serialize};
use serde_json::Value;

// === Base Protocol Messages ===

/// Base message type for DAP protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProtocolMessage {
    Request(RequestMessage),
    Response(ResponseMessage),
    Event(EventMessage),
}

/// DAP request message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestMessage {
    pub seq: i64,
    #[serde(rename = "type")]
    pub message_type: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
}

/// DAP response message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMessage {
    pub seq: i64,
    #[serde(rename = "type")]
    pub message_type: String,
    pub request_seq: i64,
    pub success: bool,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

/// DAP event message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMessage {
    pub seq: i64,
    #[serde(rename = "type")]
    pub message_type: String,
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
}

// === Request Arguments ===

/// Initialize request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeArguments {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
    #[serde(rename = "adapterID")]
    pub adapter_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(default = "default_true")]
    pub lines_start_at1: bool,
    #[serde(default = "default_true")]
    pub columns_start_at1: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_format: Option<String>,
    #[serde(default)]
    pub supports_variable_type: bool,
    #[serde(default)]
    pub supports_variable_paging: bool,
    #[serde(default)]
    pub supports_run_in_terminal_request: bool,
    #[serde(default)]
    pub supports_memory_references: bool,
    #[serde(default)]
    pub supports_progress_reporting: bool,
}

fn default_true() -> bool {
    true
}

impl Default for InitializeArguments {
    fn default() -> Self {
        Self {
            client_id: Some("debugger-cli".to_string()),
            client_name: Some("LLM Debugger CLI".to_string()),
            adapter_id: "lldb-dap".to_string(),
            locale: None,
            lines_start_at1: true,
            columns_start_at1: true,
            path_format: Some("path".to_string()),
            supports_variable_type: true,
            supports_variable_paging: true,
            supports_run_in_terminal_request: false,
            supports_memory_references: true,
            supports_progress_reporting: false,
        }
    }
}

/// Launch request arguments
///
/// This structure contains fields for multiple DAP adapters.
/// Unused fields are skipped during serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchArguments {
    pub program: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    pub stop_on_entry: bool,
    
    // === lldb-dap specific ===
    #[serde(skip_serializing_if = "Option::is_none")]
    pub init_commands: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_run_commands: Option<Vec<String>>,
    
    // === debugpy (Python) specific ===
    /// Request type: "launch" or "attach" (required by debugpy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request: Option<String>,
    /// Console type: "integratedTerminal", "internalConsole", or "externalTerminal"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub console: Option<String>,
    /// Python executable path (debugpy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python: Option<String>,
    /// Only debug user code, skip library frames (debugpy)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub just_my_code: Option<bool>,
}

/// Attach request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachArguments {
    pub pid: u32,
    // lldb-dap specific
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait_for: Option<bool>,
}

/// SetBreakpoints request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBreakpointsArguments {
    pub source: Source,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub breakpoints: Vec<SourceBreakpoint>,
}

/// SetFunctionBreakpoints request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetFunctionBreakpointsArguments {
    pub breakpoints: Vec<FunctionBreakpoint>,
}

/// Continue request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinueArguments {
    pub thread_id: i64,
    #[serde(default)]
    pub single_thread: bool,
}

/// Step request arguments (next, stepIn, stepOut)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepArguments {
    pub thread_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granularity: Option<String>,
}

/// Pause request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PauseArguments {
    pub thread_id: i64,
}

/// StackTrace request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackTraceArguments {
    pub thread_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_frame: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub levels: Option<i64>,
}

/// Scopes request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopesArguments {
    pub frame_id: i64,
}

/// Variables request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VariablesArguments {
    pub variables_reference: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<i64>,
}

/// Evaluate request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateArguments {
    pub expression: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Disconnect request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisconnectArguments {
    #[serde(default)]
    pub restart: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate_debuggee: Option<bool>,
}

// === Response Bodies ===

/// Capabilities returned by initialize response
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    #[serde(default)]
    pub supports_configuration_done_request: bool,
    #[serde(default)]
    pub supports_function_breakpoints: bool,
    #[serde(default)]
    pub supports_conditional_breakpoints: bool,
    #[serde(default)]
    pub supports_hit_conditional_breakpoints: bool,
    #[serde(default)]
    pub supports_evaluate_for_hovers: bool,
    #[serde(default)]
    pub supports_step_back: bool,
    #[serde(default)]
    pub supports_set_variable: bool,
    #[serde(default)]
    pub supports_restart_frame: bool,
    #[serde(default)]
    pub supports_restart_request: bool,
    #[serde(default)]
    pub supports_goto_targets_request: bool,
    #[serde(default)]
    pub supports_step_in_targets_request: bool,
    #[serde(default)]
    pub supports_completions_request: bool,
    #[serde(default)]
    pub supports_modules_request: bool,
    #[serde(default)]
    pub supports_data_breakpoints: bool,
    #[serde(default)]
    pub supports_read_memory_request: bool,
    #[serde(default)]
    pub supports_disassemble_request: bool,
    #[serde(default)]
    pub supports_terminate_request: bool,
}

/// SetBreakpoints response body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetBreakpointsResponseBody {
    pub breakpoints: Vec<Breakpoint>,
}

/// StackTrace response body
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackTraceResponseBody {
    pub stack_frames: Vec<StackFrame>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_frames: Option<i64>,
}

/// Threads response body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadsResponseBody {
    pub threads: Vec<Thread>,
}

/// Scopes response body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopesResponseBody {
    pub scopes: Vec<Scope>,
}

/// Variables response body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariablesResponseBody {
    pub variables: Vec<Variable>,
}

/// Evaluate response body
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateResponseBody {
    pub result: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(default)]
    pub variables_reference: i64,
}

/// Continue response body
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinueResponseBody {
    #[serde(default = "default_true")]
    pub all_threads_continued: bool,
}

// === Common Types ===

/// Source location
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_reference: Option<i64>,
}

/// Breakpoint to set at a source location
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBreakpoint {
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_message: Option<String>,
}

/// Function breakpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionBreakpoint {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
}

/// Breakpoint information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakpoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    pub verified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

/// Stack frame
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StackFrame {
    pub id: i64,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    pub line: u32,
    pub column: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub module_id: Option<Value>,
}

/// Thread
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: i64,
    pub name: String,
}

/// Scope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    pub name: String,
    pub variables_reference: i64,
    #[serde(default)]
    pub expensive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// Variable
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variable {
    pub name: String,
    pub value: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(default)]
    pub variables_reference: i64,
}

// === Event Bodies ===

/// Stopped event body
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoppedEventBody {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<i64>,
    #[serde(default)]
    pub all_threads_stopped: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hit_breakpoint_ids: Vec<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Output event body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputEventBody {
    pub category: Option<String>,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<Source>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
}

/// Thread event body
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadEventBody {
    pub reason: String,
    pub thread_id: i64,
}

/// Exited event body
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitedEventBody {
    pub exit_code: i32,
}

/// Terminated event body
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminatedEventBody {
    #[serde(default)]
    pub restart: bool,
}

// === Parsed Events ===

/// Parsed DAP event
#[derive(Debug, Clone)]
pub enum Event {
    Initialized,
    Stopped(StoppedEventBody),
    Continued { thread_id: i64, all_threads_continued: bool },
    Exited(ExitedEventBody),
    Terminated(Option<TerminatedEventBody>),
    Thread(ThreadEventBody),
    Output(OutputEventBody),
    Breakpoint { reason: String, breakpoint: Breakpoint },
    Unknown { event: String, body: Option<Value> },
}

impl Event {
    /// Parse an event from an EventMessage
    pub fn from_message(msg: &EventMessage) -> Self {
        match msg.event.as_str() {
            "initialized" => Event::Initialized,
            "stopped" => {
                if let Some(body) = &msg.body {
                    if let Ok(stopped) = serde_json::from_value(body.clone()) {
                        return Event::Stopped(stopped);
                    }
                }
                Event::Unknown {
                    event: msg.event.clone(),
                    body: msg.body.clone(),
                }
            }
            "continued" => {
                let thread_id = msg.body.as_ref()
                    .and_then(|b| b.get("threadId"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let all_threads_continued = msg.body.as_ref()
                    .and_then(|b| b.get("allThreadsContinued"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                Event::Continued { thread_id, all_threads_continued }
            }
            "exited" => {
                if let Some(body) = &msg.body {
                    if let Ok(exited) = serde_json::from_value(body.clone()) {
                        return Event::Exited(exited);
                    }
                }
                Event::Exited(ExitedEventBody { exit_code: 0 })
            }
            "terminated" => {
                let body = msg.body.as_ref()
                    .and_then(|b| serde_json::from_value(b.clone()).ok());
                Event::Terminated(body)
            }
            "thread" => {
                if let Some(body) = &msg.body {
                    if let Ok(thread) = serde_json::from_value(body.clone()) {
                        return Event::Thread(thread);
                    }
                }
                Event::Unknown {
                    event: msg.event.clone(),
                    body: msg.body.clone(),
                }
            }
            "output" => {
                if let Some(body) = &msg.body {
                    if let Ok(output) = serde_json::from_value(body.clone()) {
                        return Event::Output(output);
                    }
                }
                Event::Unknown {
                    event: msg.event.clone(),
                    body: msg.body.clone(),
                }
            }
            "breakpoint" => {
                if let Some(body) = &msg.body {
                    let reason = body.get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    if let Some(bp) = body.get("breakpoint") {
                        if let Ok(breakpoint) = serde_json::from_value(bp.clone()) {
                            return Event::Breakpoint { reason, breakpoint };
                        }
                    }
                }
                Event::Unknown {
                    event: msg.event.clone(),
                    body: msg.body.clone(),
                }
            }
            _ => Event::Unknown {
                event: msg.event.clone(),
                body: msg.body.clone(),
            },
        }
    }
}
