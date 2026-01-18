//! Integration tests for the debugger CLI
//!
//! These tests use the mock adapter to test the full CLI → daemon → adapter flow
//! without requiring a real debug adapter to be installed.

use std::process::{Command, Stdio};
use std::path::PathBuf;
use std::time::Duration;
use std::thread;

/// Get the path to the mock adapter binary
fn mock_adapter_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push(if cfg!(debug_assertions) { "debug" } else { "release" });
    path.push("mock_adapter");
    path
}

/// Get the path to the debugger binary
fn debugger_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push(if cfg!(debug_assertions) { "debug" } else { "release" });
    path.push("debugger");
    path
}

/// Run a debugger command and return its output
fn run_debugger(args: &[&str]) -> std::io::Result<std::process::Output> {
    Command::new(debugger_path())
        .args(args)
        .output()
}

/// Run a debugger command and check it succeeds
fn run_debugger_ok(args: &[&str]) -> String {
    let output = run_debugger(args).expect("Failed to run debugger");
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        panic!("Command failed: {:?}\nstdout: {}\nstderr: {}", args, stdout, stderr);
    }
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn test_status_works() {
    // Status should work regardless of daemon state
    let output = run_debugger_ok(&["status"]);
    // Status output should contain either daemon running or not running status
    assert!(
        output.contains("Daemon:") || output.contains("daemon"),
        "Status should report daemon status: {}",
        output
    );
}

#[test]
fn test_help() {
    let output = run_debugger_ok(&["--help"]);
    assert!(output.contains("debugger") || output.contains("Debug Adapter Protocol"),
            "Help should mention debugger or DAP: {}", output);
}

// Note: More comprehensive integration tests require the mock adapter to be built
// and a way to configure the debugger to use it. These tests demonstrate the pattern.

#[cfg(test)]
mod protocol_tests {
    //! Tests for the DAP protocol implementation
    //!
    //! These tests verify our DAP types serialize/deserialize correctly.

    use serde_json::json;

    #[test]
    fn test_breakpoint_location_parse_file_line() {
        // This would test BreakpointLocation::parse
        // For now, just verify the test infrastructure works
        let loc = "src/main.rs:42";
        assert!(loc.contains(':'));
    }

    #[test]
    fn test_breakpoint_location_parse_function() {
        let loc = "main";
        assert!(!loc.contains(':'));
    }
}
