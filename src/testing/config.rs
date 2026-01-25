//! Test scenario configuration types
//!
//! Defines the data structures for deserializing YAML test scenarios.

use serde::Deserialize;
use std::path::PathBuf;

/// A complete test scenario loaded from a YAML file
#[derive(Deserialize, Debug)]
pub struct TestScenario {
    /// Name of the test scenario
    pub name: String,
    /// Optional description of what the test verifies
    pub description: Option<String>,
    /// Optional setup steps to run before the test (e.g., compilation)
    pub setup: Option<Vec<SetupStep>>,
    /// Configuration for the debug target
    pub target: TargetConfig,
    /// The sequence of test steps to execute
    pub steps: Vec<TestStep>,
}

/// A setup step that runs before the test
#[derive(Deserialize, Debug)]
pub struct SetupStep {
    /// Shell command to execute
    pub shell: String,
}

/// Configuration for the debug target
#[derive(Deserialize, Debug)]
pub struct TargetConfig {
    /// Path to the program to debug
    pub program: PathBuf,
    /// Arguments to pass to the program
    pub args: Option<Vec<String>>,
    /// Debug mode: "launch" (default) or "attach"
    #[serde(default = "default_mode")]
    pub mode: String,
    /// PID to attach to (for attach mode)
    pub pid: Option<u32>,
    /// Path to file containing PID (for attach mode with setup-generated PIDs)
    pub pid_file: Option<PathBuf>,
    /// Debug adapter to use (e.g., "lldb-dap", "codelldb", "debugpy")
    pub adapter: Option<String>,
    /// Whether to stop at the program entry point
    #[serde(default)]
    pub stop_on_entry: bool,
}

fn default_mode() -> String {
    "launch".to_string()
}

/// A single test step in the execution flow
#[derive(Deserialize, Debug)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TestStep {
    /// Execute a debugger command
    Command {
        /// The command to execute (e.g., "break add main", "continue")
        command: String,
        /// Optional expectations for the command result
        expect: Option<CommandExpectation>,
    },
    /// Wait for a stop event (breakpoint, step completion, etc.)
    Await {
        /// Timeout in seconds (default: 30)
        timeout: Option<u64>,
        /// Expected stop event properties
        expect: Option<StopExpectation>,
    },
    /// Inspect local variables and make assertions
    InspectLocals {
        /// Variable assertions to check
        asserts: Vec<VariableAssertion>,
    },
    /// Inspect the call stack
    InspectStack {
        /// Frame assertions to check
        asserts: Vec<FrameAssertion>,
    },
    /// Check program output
    CheckOutput {
        /// Expected substring in output
        contains: Option<String>,
        /// Expected exact output
        equals: Option<String>,
    },
    /// Evaluate an expression
    Evaluate {
        /// Expression to evaluate
        expression: String,
        /// Expected result
        expect: Option<EvaluateExpectation>,
    },
}

/// Expectations for a command result
#[derive(Deserialize, Debug)]
pub struct CommandExpectation {
    /// Whether the command should succeed
    pub success: Option<bool>,
    /// Substring that should be in the output
    pub output_contains: Option<String>,
}

/// Expectations for a stop event
#[derive(Deserialize, Debug)]
pub struct StopExpectation {
    /// Expected stop reason (e.g., "breakpoint", "step", "exited")
    pub reason: Option<String>,
    /// Expected source file name (partial match)
    pub file: Option<String>,
    /// Expected line number
    pub line: Option<u32>,
    /// Expected exit code (for "exited" reason)
    pub exit_code: Option<i64>,
    /// Expected thread ID
    pub thread_id: Option<i64>,
}

/// Assertion for a variable
#[derive(Deserialize, Debug)]
pub struct VariableAssertion {
    /// Variable name to check
    pub name: String,
    /// Expected value (exact match)
    pub value: Option<String>,
    /// Expected value substring (partial match)
    pub value_contains: Option<String>,
    /// Expected type name
    #[serde(rename = "type")]
    pub type_name: Option<String>,
}

/// Assertion for a stack frame
#[derive(Deserialize, Debug)]
pub struct FrameAssertion {
    /// Frame index (0 = current/innermost)
    pub index: usize,
    /// Expected function name
    pub function: Option<String>,
    /// Expected source file (partial match)
    pub file: Option<String>,
    /// Expected line number
    pub line: Option<u32>,
}

/// Expectations for an evaluate result
#[derive(Deserialize, Debug)]
pub struct EvaluateExpectation {
    /// Whether the evaluation should succeed (default: true)
    /// Set to false to test error scenarios (undefined variables, syntax errors)
    pub success: Option<bool>,
    /// Expected result value
    pub result: Option<String>,
    /// Expected result substring
    pub result_contains: Option<String>,
    /// Expected type name
    #[serde(rename = "type")]
    pub type_name: Option<String>,
}
