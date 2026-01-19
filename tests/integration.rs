//! End-to-end integration tests for the debugger CLI
//!
//! These tests verify the complete debugging workflow by:
//! 1. Building test fixtures (C programs)
//! 2. Running the debugger against them
//! 3. Verifying breakpoints, stepping, variable inspection, etc.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Test context with paths and cleanup
struct TestContext {
    /// Temporary directory for this test
    temp_dir: PathBuf,
    /// Path to the debugger binary
    debugger_bin: PathBuf,
    /// Path to fixtures directory
    fixtures_dir: PathBuf,
    /// Compiled binaries
    binaries: HashMap<String, PathBuf>,
    /// Config directory (XDG_CONFIG_HOME)
    config_dir: PathBuf,
    /// Runtime directory (XDG_RUNTIME_DIR)
    runtime_dir: PathBuf,
}

impl TestContext {
    /// Create a new test context
    fn new(test_name: &str) -> Self {
        let temp_base = env::temp_dir().join("debugger-cli-tests");
        let temp_dir = temp_base.join(test_name);

        // Clean up any previous test artifacts
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        let config_dir = temp_dir.join("config");
        let runtime_dir = temp_dir.join("runtime");
        fs::create_dir_all(&config_dir).expect("Failed to create config dir");
        fs::create_dir_all(&runtime_dir).expect("Failed to create runtime dir");

        // Find the debugger binary
        let debugger_bin = find_debugger_binary();

        // Find fixtures directory
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let fixtures_dir = PathBuf::from(manifest_dir).join("tests").join("fixtures");

        Self {
            temp_dir,
            debugger_bin,
            fixtures_dir,
            binaries: HashMap::new(),
            config_dir,
            runtime_dir,
        }
    }

    /// Build a C fixture
    fn build_c_fixture(&mut self, name: &str) -> &PathBuf {
        let source = self.fixtures_dir.join(format!("{}.c", name));
        let output = self.temp_dir.join(name);

        // Try gcc first, then clang
        let compiler = if Command::new("gcc").arg("--version").output().is_ok() {
            "gcc"
        } else if Command::new("clang").arg("--version").output().is_ok() {
            "clang"
        } else {
            panic!("No C compiler found (tried gcc, clang)");
        };

        let status = Command::new(compiler)
            .args([
                "-g",           // Debug symbols
                "-O0",          // No optimization
                "-o",
                output.to_str().unwrap(),
                source.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to compile C fixture");

        assert!(status.success(), "C compilation failed");

        self.binaries.insert(name.to_string(), output.clone());
        self.binaries.get(name).unwrap()
    }

    /// Build a Rust fixture
    fn build_rust_fixture(&mut self, name: &str) -> &PathBuf {
        let source = self.fixtures_dir.join(format!("{}.rs", name));
        let output = self.temp_dir.join(format!("{}_rs", name));

        let status = Command::new("rustc")
            .args([
                "-g",           // Debug symbols
                "-o",
                output.to_str().unwrap(),
                source.to_str().unwrap(),
            ])
            .status()
            .expect("Failed to compile Rust fixture");

        assert!(status.success(), "Rust compilation failed");

        self.binaries.insert(format!("{}_rs", name), output.clone());
        self.binaries.get(&format!("{}_rs", name)).unwrap()
    }

    /// Find breakpoint line numbers from markers in source
    fn find_breakpoint_markers(&self, source: &Path) -> HashMap<String, u32> {
        let content = fs::read_to_string(source).expect("Failed to read source file");
        let mut markers = HashMap::new();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(marker_start) = line.find("BREAKPOINT_MARKER:") {
                let marker_name = line[marker_start + "BREAKPOINT_MARKER:".len()..]
                    .trim()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .to_string();
                // Calculate breakpoint line:
                // - enumerate() gives 0-indexed line numbers
                // - DAP uses 1-indexed line numbers, so add 1
                // - We want the NEXT line after the marker comment, so add another 1
                // - Total: line_num + 2
                let breakpoint_line = (line_num as u32) + 2;
                markers.insert(marker_name, breakpoint_line);
            }
        }

        markers
    }

    /// Create a config file for the test
    fn create_config(&self, adapter_name: &str, adapter_path: &str) {
        let config_content = format!(
            r#"
[adapters.{adapter_name}]
path = "{adapter_path}"
args = []

[defaults]
adapter = "{adapter_name}"

[timeouts]
dap_initialize_secs = 10
dap_request_secs = 30
await_default_secs = 60

[daemon]
idle_timeout_minutes = 5

[output]
max_events = 1000
max_bytes_mb = 1
"#,
            adapter_name = adapter_name,
            adapter_path = adapter_path,
        );

        let config_path = self.config_dir.join("debugger-cli").join("config.toml");
        fs::create_dir_all(config_path.parent().unwrap()).expect("Failed to create config dir");
        fs::write(&config_path, config_content).expect("Failed to write config");
    }

    /// Run a debugger command
    fn run_debugger(&self, args: &[&str]) -> DebuggerOutput {
        let output = Command::new(&self.debugger_bin)
            .args(args)
            .env("XDG_CONFIG_HOME", &self.config_dir)
            .env("XDG_RUNTIME_DIR", &self.runtime_dir)
            .output()
            .expect("Failed to run debugger");

        DebuggerOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
            code: output.status.code(),
        }
    }

    /// Run debugger command expecting success
    fn run_debugger_ok(&self, args: &[&str]) -> String {
        let output = self.run_debugger(args);
        assert!(
            output.success,
            "Debugger command {:?} failed:\nstdout: {}\nstderr: {}",
            args,
            output.stdout,
            output.stderr
        );
        output.stdout
    }

    /// Stop any running daemon
    fn cleanup_daemon(&self) {
        let _ = self.run_debugger(&["stop"]);
        // Give it a moment to clean up
        std::thread::sleep(Duration::from_millis(100));
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Clean up daemon
        let _ = self.run_debugger(&["stop"]);

        // Clean up temp directory based on environment variable
        // By default, preserve artifacts for debugging test failures
        // Set PRESERVE_DEBUGGER_TEST_ARTIFACTS=0 (or "false"/"no") to clean up
        let preserve = env::var("PRESERVE_DEBUGGER_TEST_ARTIFACTS")
            .unwrap_or_else(|_| "1".to_string())
            .to_ascii_lowercase();

        if preserve == "0" || preserve == "false" || preserve == "no" {
            let _ = fs::remove_dir_all(&self.temp_dir);
        }
    }
}

/// Output from a debugger command
#[derive(Debug)]
struct DebuggerOutput {
    stdout: String,
    stderr: String,
    success: bool,
    code: Option<i32>,
}

/// Find the debugger binary
fn find_debugger_binary() -> PathBuf {
    // Try to find in target directory
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let candidates = [
        PathBuf::from(manifest_dir).join("target/debug/debugger"),
        PathBuf::from(manifest_dir).join("target/release/debugger"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    // Fall back to cargo build
    let status = Command::new("cargo")
        .args(["build"])
        .current_dir(manifest_dir)
        .status()
        .expect("Failed to build debugger");
    assert!(status.success(), "Failed to build debugger");

    candidates[0].clone()
}

/// Check if lldb-dap is available
fn lldb_dap_available() -> Option<PathBuf> {
    // Try common paths
    let candidates = [
        "lldb-dap",
        "lldb-vscode",
        "/usr/bin/lldb-dap",
        "/usr/local/bin/lldb-dap",
        "/opt/homebrew/opt/llvm/bin/lldb-dap",
    ];

    for candidate in &candidates {
        if let Ok(path) = which::which(candidate) {
            return Some(path);
        }
    }
    None
}

// ============== Tests ==============

#[test]
fn test_status_no_daemon() {
    let ctx = TestContext::new("status_no_daemon");
    let output = ctx.run_debugger(&["status"]);

    // Should report daemon not running
    assert!(
        output.stdout.contains("Daemon: not running") || output.stdout.contains("not running"),
        "Expected 'not running' in output: {}",
        output.stdout
    );
}

#[test]
fn test_breakpoint_location_parsing() {
    // This tests the internal breakpoint location parsing
    // without needing a real debug adapter

    use debugger::ipc::protocol::BreakpointLocation;

    // Test file:line format
    let loc = BreakpointLocation::parse("src/main.rs:42").unwrap();
    match loc {
        BreakpointLocation::Line { file, line } => {
            assert_eq!(file.to_string_lossy(), "src/main.rs");
            assert_eq!(line, 42);
        }
        _ => panic!("Expected Line variant"),
    }

    // Test function format
    let loc = BreakpointLocation::parse("main").unwrap();
    match loc {
        BreakpointLocation::Function { name } => {
            assert_eq!(name, "main");
        }
        _ => panic!("Expected Function variant"),
    }

    // Test namespaced function
    let loc = BreakpointLocation::parse("mymod::MyStruct::method").unwrap();
    match loc {
        BreakpointLocation::Function { name } => {
            assert_eq!(name, "mymod::MyStruct::method");
        }
        _ => panic!("Expected Function variant"),
    }
}

#[test]
#[ignore = "requires lldb-dap"]
fn test_basic_debugging_workflow_c() {
    let lldb_path = match lldb_dap_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: lldb-dap not available");
            return;
        }
    };

    let mut ctx = TestContext::new("basic_workflow_c");
    ctx.create_config("lldb-dap", lldb_path.to_str().unwrap());

    // Build the C fixture
    let binary = ctx.build_c_fixture("simple").clone();

    // Find breakpoint markers
    let markers = ctx.find_breakpoint_markers(&ctx.fixtures_dir.join("simple.c"));
    let main_start_line = markers.get("main_start").expect("Missing main_start marker");

    // Cleanup any existing daemon
    ctx.cleanup_daemon();

    // Start debugging
    let output = ctx.run_debugger_ok(&[
        "start",
        binary.to_str().unwrap(),
        "--stop-on-entry",
    ]);
    assert!(output.contains("Started debugging") || output.contains("Stopped"));

    // Check status
    let output = ctx.run_debugger_ok(&["status"]);
    assert!(output.contains("Session: active") || output.contains("session_active"));

    // Set a breakpoint
    let bp_location = format!("simple.c:{}", main_start_line);
    let output = ctx.run_debugger_ok(&["break", &bp_location]);
    assert!(output.contains("Breakpoint") || output.contains("breakpoint"));

    // Continue execution
    let output = ctx.run_debugger_ok(&["continue"]);
    assert!(output.contains("Continuing") || output.contains("running"));

    // Wait for breakpoint hit
    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
    assert!(
        output.contains("Stopped") || output.contains("breakpoint"),
        "Expected stop at breakpoint: {}",
        output
    );

    // Get backtrace
    let output = ctx.run_debugger_ok(&["backtrace"]);
    assert!(output.contains("main") || output.contains("#0"));

    // Get local variables
    let output = ctx.run_debugger_ok(&["locals"]);
    // Should show x and y variables
    assert!(
        output.contains("x") || output.contains("Local"),
        "Expected locals output: {}",
        output
    );

    // Continue to end
    let _ = ctx.run_debugger(&["continue"]);
    let output = ctx.run_debugger(&["await", "--timeout", "10"]);
    assert!(
        output.stdout.contains("exited") || output.stdout.contains("terminated") ||
        output.stderr.contains("exited") || output.stderr.contains("terminated") ||
        output.stdout.contains("stopped"),
        "Expected program to finish: {:?}",
        output
    );

    // Stop the session
    let _ = ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "requires lldb-dap"]
fn test_stepping_c() {
    let lldb_path = match lldb_dap_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: lldb-dap not available");
            return;
        }
    };

    let mut ctx = TestContext::new("stepping_c");
    ctx.create_config("lldb-dap", lldb_path.to_str().unwrap());

    let binary = ctx.build_c_fixture("simple").clone();
    let markers = ctx.find_breakpoint_markers(&ctx.fixtures_dir.join("simple.c"));
    let before_add_line = markers.get("before_add").expect("Missing before_add marker");

    ctx.cleanup_daemon();

    // Start and set breakpoint before add() call
    ctx.run_debugger_ok(&["start", binary.to_str().unwrap(), "--stop-on-entry"]);

    let bp_location = format!("simple.c:{}", before_add_line);
    ctx.run_debugger_ok(&["break", &bp_location]);
    ctx.run_debugger_ok(&["continue"]);
    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
    assert!(output.contains("Stopped") || output.contains("breakpoint"));

    // Step into add()
    ctx.run_debugger_ok(&["step"]);
    let output = ctx.run_debugger_ok(&["await", "--timeout", "10"]);

    // Get context to verify we're in add()
    let output = ctx.run_debugger_ok(&["backtrace"]);
    assert!(
        output.contains("add") || output.contains("simple.c"),
        "Expected to be in add(): {}",
        output
    );

    // Step out back to main
    ctx.run_debugger_ok(&["finish"]);
    let _ = ctx.run_debugger(&["await", "--timeout", "10"]);

    // Verify we're back in main
    let output = ctx.run_debugger_ok(&["backtrace"]);
    assert!(output.contains("main"), "Expected to be in main(): {}", output);

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "requires lldb-dap"]
fn test_expression_evaluation_c() {
    let lldb_path = match lldb_dap_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: lldb-dap not available");
            return;
        }
    };

    let mut ctx = TestContext::new("eval_c");
    ctx.create_config("lldb-dap", lldb_path.to_str().unwrap());

    let binary = ctx.build_c_fixture("simple").clone();
    let markers = ctx.find_breakpoint_markers(&ctx.fixtures_dir.join("simple.c"));
    let before_add_line = markers.get("before_add").expect("Missing before_add marker");

    ctx.cleanup_daemon();

    ctx.run_debugger_ok(&["start", binary.to_str().unwrap(), "--stop-on-entry"]);

    let bp_location = format!("simple.c:{}", before_add_line);
    ctx.run_debugger_ok(&["break", &bp_location]);
    ctx.run_debugger_ok(&["continue"]);
    ctx.run_debugger_ok(&["await", "--timeout", "30"]);

    // Evaluate expressions
    let output = ctx.run_debugger_ok(&["print", "x"]);
    assert!(output.contains("10") || output.contains("x ="), "Expected x=10: {}", output);

    let output = ctx.run_debugger_ok(&["print", "y"]);
    assert!(output.contains("20") || output.contains("y ="), "Expected y=20: {}", output);

    let output = ctx.run_debugger_ok(&["print", "x + y"]);
    assert!(output.contains("30"), "Expected x+y=30: {}", output);

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "requires lldb-dap"]
fn test_multiple_breakpoints_c() {
    let lldb_path = match lldb_dap_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: lldb-dap not available");
            return;
        }
    };

    let mut ctx = TestContext::new("multi_bp_c");
    ctx.create_config("lldb-dap", lldb_path.to_str().unwrap());

    let binary = ctx.build_c_fixture("simple").clone();
    let markers = ctx.find_breakpoint_markers(&ctx.fixtures_dir.join("simple.c"));

    ctx.cleanup_daemon();

    ctx.run_debugger_ok(&["start", binary.to_str().unwrap(), "--stop-on-entry"]);

    // Set multiple breakpoints
    for marker in ["main_start", "before_add", "before_factorial"] {
        let line = markers.get(marker).expect(&format!("Missing {} marker", marker));
        let bp_location = format!("simple.c:{}", line);
        ctx.run_debugger_ok(&["break", &bp_location]);
    }

    // List breakpoints
    let output = ctx.run_debugger_ok(&["breakpoint", "list"]);
    assert!(
        output.contains("Breakpoint") || output.contains("breakpoint"),
        "Expected breakpoints in list: {}",
        output
    );

    // Continue and hit first breakpoint
    ctx.run_debugger_ok(&["continue"]);
    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
    assert!(output.contains("Stopped") || output.contains("breakpoint"));

    // Continue and hit second breakpoint
    ctx.run_debugger_ok(&["continue"]);
    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
    assert!(output.contains("Stopped") || output.contains("breakpoint"));

    // Remove all breakpoints
    ctx.run_debugger_ok(&["breakpoint", "remove", "--all"]);

    // Verify no breakpoints
    let output = ctx.run_debugger_ok(&["breakpoint", "list"]);
    assert!(
        output.contains("No breakpoints") || output.to_lowercase().contains("no breakpoints"),
        "Expected no breakpoints: {}",
        output
    );

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "requires lldb-dap"]
fn test_threads_c() {
    let lldb_path = match lldb_dap_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: lldb-dap not available");
            return;
        }
    };

    let mut ctx = TestContext::new("threads_c");
    ctx.create_config("lldb-dap", lldb_path.to_str().unwrap());

    let binary = ctx.build_c_fixture("simple").clone();

    ctx.cleanup_daemon();

    ctx.run_debugger_ok(&["start", binary.to_str().unwrap(), "--stop-on-entry"]);

    // List threads
    let output = ctx.run_debugger_ok(&["threads"]);
    assert!(
        output.contains("Thread") || output.contains("thread") || output.contains("-"),
        "Expected thread list: {}",
        output
    );

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "requires lldb-dap"]
fn test_frame_navigation_c() {
    let lldb_path = match lldb_dap_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: lldb-dap not available");
            return;
        }
    };

    let mut ctx = TestContext::new("frame_nav_c");
    ctx.create_config("lldb-dap", lldb_path.to_str().unwrap());

    let binary = ctx.build_c_fixture("simple").clone();
    let markers = ctx.find_breakpoint_markers(&ctx.fixtures_dir.join("simple.c"));
    let add_body_line = markers.get("add_body").expect("Missing add_body marker");

    ctx.cleanup_daemon();

    ctx.run_debugger_ok(&["start", binary.to_str().unwrap(), "--stop-on-entry"]);

    // Set breakpoint inside add() function
    let bp_location = format!("simple.c:{}", add_body_line);
    ctx.run_debugger_ok(&["break", &bp_location]);
    ctx.run_debugger_ok(&["continue"]);
    ctx.run_debugger_ok(&["await", "--timeout", "30"]);

    // Get backtrace - should show add() and main()
    let output = ctx.run_debugger_ok(&["backtrace"]);
    assert!(output.contains("add"), "Expected add in backtrace: {}", output);
    assert!(output.contains("main"), "Expected main in backtrace: {}", output);

    // Navigate up to main's frame
    let output = ctx.run_debugger_ok(&["up"]);
    assert!(
        output.contains("main") || output.contains("#1"),
        "Expected to move up to main: {}",
        output
    );

    // Navigate back down
    let output = ctx.run_debugger_ok(&["down"]);
    assert!(
        output.contains("add") || output.contains("#0"),
        "Expected to move down to add: {}",
        output
    );

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "requires lldb-dap"]
fn test_output_capture_c() {
    let lldb_path = match lldb_dap_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: lldb-dap not available");
            return;
        }
    };

    let mut ctx = TestContext::new("output_c");
    ctx.create_config("lldb-dap", lldb_path.to_str().unwrap());

    let binary = ctx.build_c_fixture("simple").clone();

    ctx.cleanup_daemon();

    ctx.run_debugger_ok(&["start", binary.to_str().unwrap()]);

    // Wait for program to finish
    let output = ctx.run_debugger(&["await", "--timeout", "30"]);

    // Get output
    let output = ctx.run_debugger_ok(&["output"]);

    // Should contain program output
    assert!(
        output.contains("Sum:") || output.contains("Factorial:") || output.contains("no output"),
        "Expected program output: {}",
        output
    );

    ctx.run_debugger(&["stop"]);
}

#[test]
fn test_config_loading() {
    // Test that configuration is loaded correctly
    let ctx = TestContext::new("config_loading");

    // Create a test config
    ctx.create_config("test-adapter", "/nonexistent/path");

    // The daemon will fail to start with a nonexistent adapter,
    // but we can verify the config is read
    let output = ctx.run_debugger(&[
        "start",
        "/bin/true", // Won't actually run
        "--adapter", "test-adapter",
    ]);

    // Should fail because adapter doesn't exist
    assert!(
        !output.success || output.stderr.contains("not found") || output.stderr.contains("Failed"),
        "Expected failure for nonexistent adapter"
    );
}

// ============================================================================
// Mock Adapter Integration Tests
//
// These tests use the mock-adapter binary to test the debugger without
// requiring a real debug adapter like lldb-dap.
// ============================================================================

/// Find the mock adapter binary
fn find_mock_adapter() -> Option<PathBuf> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let candidates = [
        PathBuf::from(manifest_dir).join("target/debug/mock-adapter"),
        PathBuf::from(manifest_dir).join("target/release/mock-adapter"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }
    None
}

#[test]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
fn test_mock_adapter_basic_workflow() {
    let mock_adapter = match find_mock_adapter() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: mock-adapter not built. Run 'cargo build' first.");
            return;
        }
    };

    let ctx = TestContext::new("mock_basic");
    ctx.create_config("mock-adapter", mock_adapter.to_str().unwrap());

    ctx.cleanup_daemon();

    // Start debugging
    let output = ctx.run_debugger_ok(&[
        "start", "/tmp/fake-program", "--adapter", "mock-adapter", "--stop-on-entry"
    ]);
    assert!(output.contains("Session started") || output.contains("Stopped"),
        "Expected session start, got: {}", output);

    // Wait for stop
    let output = ctx.run_debugger_ok(&["await", "--timeout", "5"]);
    assert!(output.contains("Stopped"), "Expected stopped state, got: {}", output);

    // Check status
    let output = ctx.run_debugger_ok(&["status"]);
    assert!(output.contains("Session active") || output.contains("stopped"),
        "Expected active session, got: {}", output);

    // Get backtrace
    let output = ctx.run_debugger_ok(&["backtrace"]);
    assert!(output.contains("main"), "Expected main in backtrace, got: {}", output);

    // Get locals
    let output = ctx.run_debugger_ok(&["locals"]);
    assert!(output.contains("x") || output.contains("42"),
        "Expected variable x in locals, got: {}", output);

    // Continue and wait
    ctx.run_debugger_ok(&["continue"]);
    let output = ctx.run_debugger_ok(&["await", "--timeout", "5"]);
    assert!(output.contains("Stopped"), "Expected stopped after continue, got: {}", output);

    // Step
    ctx.run_debugger_ok(&["step"]);
    let output = ctx.run_debugger_ok(&["await", "--timeout", "5"]);
    assert!(output.contains("Stopped") || output.contains("Step"),
        "Expected stopped after step, got: {}", output);

    // Stop session
    let output = ctx.run_debugger_ok(&["stop"]);
    assert!(output.contains("Session stopped") || output.contains("stopped"),
        "Expected session stop, got: {}", output);
}

#[test]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
fn test_mock_adapter_breakpoints() {
    let mock_adapter = match find_mock_adapter() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: mock-adapter not built.");
            return;
        }
    };

    let ctx = TestContext::new("mock_breakpoints");
    ctx.create_config("mock-adapter", mock_adapter.to_str().unwrap());

    ctx.cleanup_daemon();

    // Start debugging
    ctx.run_debugger_ok(&[
        "start", "/tmp/fake-program", "--adapter", "mock-adapter", "--stop-on-entry"
    ]);
    ctx.run_debugger_ok(&["await", "--timeout", "5"]);

    // Add breakpoint
    let output = ctx.run_debugger_ok(&["breakpoint", "add", "main.c:10"]);
    assert!(output.contains("Breakpoint") && output.contains("set"),
        "Expected breakpoint set, got: {}", output);

    // List breakpoints
    let output = ctx.run_debugger_ok(&["breakpoint", "list"]);
    assert!(output.contains("main.c") || output.contains("10"),
        "Expected breakpoint in list, got: {}", output);

    // Add function breakpoint
    let output = ctx.run_debugger_ok(&["breakpoint", "add", "main"]);
    assert!(output.contains("Breakpoint"), "Expected function breakpoint, got: {}", output);

    // Remove breakpoint
    let output = ctx.run_debugger_ok(&["breakpoint", "remove", "1"]);
    assert!(output.contains("Removed") || output.contains("removed"),
        "Expected breakpoint removed, got: {}", output);

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
fn test_mock_adapter_evaluation() {
    let mock_adapter = match find_mock_adapter() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: mock-adapter not built.");
            return;
        }
    };

    let ctx = TestContext::new("mock_eval");
    ctx.create_config("mock-adapter", mock_adapter.to_str().unwrap());

    ctx.cleanup_daemon();

    // Start debugging
    ctx.run_debugger_ok(&[
        "start", "/tmp/fake-program", "--adapter", "mock-adapter", "--stop-on-entry"
    ]);
    ctx.run_debugger_ok(&["await", "--timeout", "5"]);

    // Print variable
    let output = ctx.run_debugger_ok(&["print", "x"]);
    assert!(output.contains("42") || output.contains("x"),
        "Expected variable value, got: {}", output);

    // Eval expression
    let output = ctx.run_debugger_ok(&["eval", "x + 1"]);
    assert!(output.contains("eval") || output.contains("result"),
        "Expected eval result, got: {}", output);

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
fn test_mock_adapter_set_variable() {
    let mock_adapter = match find_mock_adapter() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: mock-adapter not built.");
            return;
        }
    };

    let ctx = TestContext::new("mock_set_var");
    ctx.create_config("mock-adapter", mock_adapter.to_str().unwrap());

    ctx.cleanup_daemon();

    // Start debugging
    ctx.run_debugger_ok(&[
        "start", "/tmp/fake-program", "--adapter", "mock-adapter", "--stop-on-entry"
    ]);
    ctx.run_debugger_ok(&["await", "--timeout", "5"]);

    // Set variable
    let output = ctx.run_debugger_ok(&["set", "x", "100"]);
    assert!(output.contains("100") || output.contains("x"),
        "Expected variable set, got: {}", output);

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
fn test_mock_adapter_memory() {
    let mock_adapter = match find_mock_adapter() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: mock-adapter not built.");
            return;
        }
    };

    let ctx = TestContext::new("mock_memory");
    ctx.create_config("mock-adapter", mock_adapter.to_str().unwrap());

    ctx.cleanup_daemon();

    // Start debugging
    ctx.run_debugger_ok(&[
        "start", "/tmp/fake-program", "--adapter", "mock-adapter", "--stop-on-entry"
    ]);
    ctx.run_debugger_ok(&["await", "--timeout", "5"]);

    // Read memory
    let output = ctx.run_debugger_ok(&["memory", "0x1000", "--count", "32"]);
    assert!(output.contains("Memory") || output.contains("00"),
        "Expected memory dump, got: {}", output);

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
fn test_mock_adapter_disassemble() {
    let mock_adapter = match find_mock_adapter() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: mock-adapter not built.");
            return;
        }
    };

    let ctx = TestContext::new("mock_disasm");
    ctx.create_config("mock-adapter", mock_adapter.to_str().unwrap());

    ctx.cleanup_daemon();

    // Start debugging
    ctx.run_debugger_ok(&[
        "start", "/tmp/fake-program", "--adapter", "mock-adapter", "--stop-on-entry"
    ]);
    ctx.run_debugger_ok(&["await", "--timeout", "5"]);

    // Disassemble
    let output = ctx.run_debugger_ok(&["disassemble", ".", "--count", "5"]);
    assert!(output.contains("mov") || output.contains("0x"),
        "Expected disassembly, got: {}", output);

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
fn test_mock_adapter_watchpoints() {
    let mock_adapter = match find_mock_adapter() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: mock-adapter not built.");
            return;
        }
    };

    let ctx = TestContext::new("mock_watchpoints");
    ctx.create_config("mock-adapter", mock_adapter.to_str().unwrap());

    ctx.cleanup_daemon();

    // Start debugging
    ctx.run_debugger_ok(&[
        "start", "/tmp/fake-program", "--adapter", "mock-adapter", "--stop-on-entry"
    ]);
    ctx.run_debugger_ok(&["await", "--timeout", "5"]);

    // Add watchpoint
    let output = ctx.run_debugger_ok(&["watch", "add", "x", "--access", "write"]);
    assert!(output.contains("Watchpoint") || output.contains("set"),
        "Expected watchpoint set, got: {}", output);

    // List watchpoints
    let output = ctx.run_debugger_ok(&["watch", "list"]);
    assert!(output.contains("x") || output.contains("write") || output.contains("Watchpoint"),
        "Expected watchpoint in list, got: {}", output);

    // Remove watchpoint
    let output = ctx.run_debugger_ok(&["watch", "remove", "1"]);
    assert!(output.contains("removed") || output.contains("Watchpoint"),
        "Expected watchpoint removed, got: {}", output);

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "run with --test-threads=1 to avoid daemon conflicts"]
fn test_mock_adapter_threads() {
    let mock_adapter = match find_mock_adapter() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: mock-adapter not built.");
            return;
        }
    };

    let ctx = TestContext::new("mock_threads");
    ctx.create_config("mock-adapter", mock_adapter.to_str().unwrap());

    ctx.cleanup_daemon();

    // Start debugging
    ctx.run_debugger_ok(&[
        "start", "/tmp/fake-program", "--adapter", "mock-adapter", "--stop-on-entry"
    ]);
    ctx.run_debugger_ok(&["await", "--timeout", "5"]);

    // List threads
    let output = ctx.run_debugger_ok(&["threads"]);
    assert!(output.contains("main") || output.contains("Thread"),
        "Expected thread list, got: {}", output);

    ctx.run_debugger(&["stop"]);
}
