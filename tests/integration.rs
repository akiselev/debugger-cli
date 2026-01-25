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
        self.create_config_with_args(adapter_name, adapter_path, &[]);
    }

    /// Create a config file for a TCP adapter
    fn create_config_with_tcp(
        &self,
        adapter_name: &str,
        adapter_path: &str,
        args: &[&str],
        spawn_style: &str,
    ) {
        let args_str = args.iter()
            .map(|a| format!("\"{}\"", a))
            .collect::<Vec<_>>()
            .join(", ");
        let config_content = format!(
            r#"
[adapters.{adapter_name}]
path = "{adapter_path}"
args = [{args_str}]
transport = "tcp"
spawn_style = "{spawn_style}"

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
            args_str = args_str,
            spawn_style = spawn_style,
        );

        let config_path = self.config_dir.join("debugger-cli").join("config.toml");
        fs::create_dir_all(config_path.parent().unwrap()).expect("Failed to create config dir");
        fs::write(&config_path, config_content).expect("Failed to write config");
    }

    /// Create a config file for the test with custom args
    fn create_config_with_args(&self, adapter_name: &str, adapter_path: &str, args: &[&str]) {
        let args_str = args.iter()
            .map(|a| format!("\"{}\"", a))
            .collect::<Vec<_>>()
            .join(", ");
        let config_content = format!(
            r#"
[adapters.{adapter_name}]
path = "{adapter_path}"
args = [{args_str}]

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
            args_str = args_str,
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

/// Checks if GDB ≥14.1 is available for testing
///
/// Returns path only if version meets DAP support requirement
fn gdb_available() -> Option<PathBuf> {
    use debugger::setup::adapters::gdb_common::{parse_gdb_version, is_gdb_version_sufficient};

    let path = which::which("gdb").ok()?;

    let output = std::process::Command::new(&path)
        .arg("--version")
        .output()
        .ok()?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = parse_gdb_version(&stdout)?;

        if is_gdb_version_sufficient(&version) {
            return Some(path);
        }
    }

    None
}

/// Checks if cuda-gdb is available for testing
///
/// Uses same path search as CudaGdbInstaller::find_cuda_gdb()
fn cuda_gdb_available() -> Option<PathBuf> {
    let default_path = PathBuf::from("/usr/local/cuda/bin/cuda-gdb");
    if default_path.exists() {
        return Some(default_path);
    }

    if let Ok(cuda_home) = std::env::var("CUDA_HOME") {
        let cuda_home_path = PathBuf::from(cuda_home).join("bin/cuda-gdb");
        if cuda_home_path.exists() {
            return Some(cuda_home_path);
        }
    }

    which::which("cuda-gdb").ok()
}

/// Checks if js-debug is available for testing
fn js_debug_available() -> Option<PathBuf> {
    let adapter_dir = debugger::setup::installer::adapters_dir().join("js-debug");
    let dap_path = adapter_dir.join("node_modules/js-debug/src/dapDebugServer.js");
    if dap_path.exists() {
        return Some(dap_path);
    }
    let dap_path = adapter_dir.join("node_modules/js-debug/dist/src/dapDebugServer.js");
    if dap_path.exists() {
        return Some(dap_path);
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
#[ignore = "GDB DAP mode has different stopOnEntry behavior than LLDB"]
fn test_basic_debugging_workflow_c_gdb() {
    let gdb_path = match gdb_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: GDB ≥14.1 not available");
            return;
        }
    };

    let mut ctx = TestContext::new("basic_workflow_c_gdb");
    ctx.create_config_with_args("gdb", gdb_path.to_str().unwrap(), &["-i=dap"]);

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

    // Get local variables
    let output = ctx.run_debugger_ok(&["locals"]);
    assert!(
        output.contains("x") || output.contains("Local"),
        "Expected locals output: {}",
        output
    );

    // Stop the session
    let _ = ctx.run_debugger(&["stop"]);
}

#[test]
fn test_cuda_gdb_adapter_available() {
    let cuda_gdb_path = match cuda_gdb_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: CUDA-GDB not available");
            return;
        }
    };

    assert!(cuda_gdb_path.exists(), "CUDA-GDB path should exist");
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
    let _output = ctx.run_debugger_ok(&["await", "--timeout", "10"]);

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
    let _output = ctx.run_debugger(&["await", "--timeout", "30"]);

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

#[test]
#[ignore = "requires js-debug"]
fn test_basic_debugging_workflow_js() {
    let js_debug_path = match js_debug_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: js-debug not available");
            eprintln!("Install with: debugger install js-debug");
            return;
        }
    };

    let node_path = match which::which("node") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("Skipping test: Node.js not available");
            return;
        }
    };

    let ctx = TestContext::new("basic_workflow_js");
    ctx.create_config_with_tcp(
        "js-debug",
        node_path.to_str().unwrap(),
        &[js_debug_path.to_str().unwrap()],
        "tcp-port-arg",
    );

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let js_fixture = PathBuf::from(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("simple.js");

    let markers = ctx.find_breakpoint_markers(&js_fixture);
    let main_start_line = markers.get("main_start").expect("Missing main_start marker");

    ctx.cleanup_daemon();

    let output = ctx.run_debugger_ok(&[
        "start",
        js_fixture.to_str().unwrap(),
        "--stop-on-entry",
    ]);
    assert!(output.contains("Started debugging") || output.contains("Stopped"));

    let bp_location = format!("simple.js:{}", main_start_line);
    let output = ctx.run_debugger_ok(&["break", &bp_location]);
    assert!(output.contains("Breakpoint") || output.contains("breakpoint"));

    let output = ctx.run_debugger_ok(&["continue"]);
    assert!(output.contains("Continuing") || output.contains("running"));

    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
    assert!(
        output.contains("Stopped") || output.contains("breakpoint"),
        "Expected stop at breakpoint: {}",
        output
    );

    let output = ctx.run_debugger_ok(&["backtrace"]);
    assert!(output.contains("main") || output.contains("#0"));

    let output = ctx.run_debugger_ok(&["locals"]);
    assert!(
        output.contains("x") || output.contains("Local"),
        "Expected locals output: {}",
        output
    );

    let _ = ctx.run_debugger(&["continue"]);
    let output = ctx.run_debugger(&["await", "--timeout", "10"]);
    assert!(
        output.stdout.contains("exited") || output.stdout.contains("terminated") ||
        output.stderr.contains("exited") || output.stderr.contains("terminated") ||
        output.stdout.contains("stopped"),
        "Expected program to finish: {:?}",
        output
    );

    let _ = ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "requires js-debug"]
fn test_basic_debugging_workflow_ts() {
    let js_debug_path = match js_debug_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: js-debug not available");
            eprintln!("Install with: debugger install js-debug");
            return;
        }
    };

    let node_path = match which::which("node") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("Skipping test: Node.js not available");
            return;
        }
    };

    let ctx = TestContext::new("basic_workflow_ts");
    ctx.create_config_with_tcp(
        "js-debug",
        node_path.to_str().unwrap(),
        &[js_debug_path.to_str().unwrap()],
        "tcp-port-arg",
    );

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let ts_fixture = PathBuf::from(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("dist")
        .join("simple.js");

    if !ts_fixture.exists() {
        eprintln!("Skipping test: TypeScript fixture not compiled");
        eprintln!("Run: cd tests/fixtures && npx tsc simple.ts --outDir dist --sourceMap");
        return;
    }

    let markers = ctx.find_breakpoint_markers(&PathBuf::from(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("simple.ts"));
    let main_start_line = markers.get("main_start").expect("Missing main_start marker");

    ctx.cleanup_daemon();

    let output = ctx.run_debugger_ok(&[
        "start",
        ts_fixture.to_str().unwrap(),
        "--stop-on-entry",
    ]);
    assert!(output.contains("Started debugging") || output.contains("Stopped"));

    let bp_location = format!("simple.ts:{}", main_start_line);
    let output = ctx.run_debugger_ok(&["break", &bp_location]);
    assert!(output.contains("Breakpoint") || output.contains("breakpoint"));

    let output = ctx.run_debugger_ok(&["continue"]);
    assert!(output.contains("Continuing") || output.contains("running"));

    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
    assert!(
        output.contains("Stopped") || output.contains("breakpoint"),
        "Expected stop at breakpoint: {}",
        output
    );

    let output = ctx.run_debugger_ok(&["backtrace"]);
    assert!(output.contains("main") || output.contains("#0"));

    let _ = ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "requires js-debug"]
fn test_stepping_js() {
    let js_debug_path = match js_debug_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: js-debug not available");
            return;
        }
    };

    let node_path = match which::which("node") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("Skipping test: Node.js not available");
            return;
        }
    };

    let ctx = TestContext::new("stepping_js");
    ctx.create_config_with_tcp(
        "js-debug",
        node_path.to_str().unwrap(),
        &[js_debug_path.to_str().unwrap()],
        "tcp-port-arg",
    );

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let js_fixture = PathBuf::from(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("simple.js");

    let markers = ctx.find_breakpoint_markers(&js_fixture);
    let before_add_line = markers.get("before_add").expect("Missing before_add marker");

    ctx.cleanup_daemon();

    ctx.run_debugger_ok(&["start", js_fixture.to_str().unwrap(), "--stop-on-entry"]);

    let bp_location = format!("simple.js:{}", before_add_line);
    ctx.run_debugger_ok(&["break", &bp_location]);
    ctx.run_debugger_ok(&["continue"]);
    let output = ctx.run_debugger_ok(&["await", "--timeout", "30"]);
    assert!(output.contains("Stopped") || output.contains("breakpoint"));

    ctx.run_debugger_ok(&["step"]);
    let _output = ctx.run_debugger_ok(&["await", "--timeout", "10"]);

    let output = ctx.run_debugger_ok(&["backtrace"]);
    assert!(
        output.contains("add") || output.contains("simple.js"),
        "Expected to be in add(): {}",
        output
    );

    ctx.run_debugger_ok(&["finish"]);
    let _ = ctx.run_debugger(&["await", "--timeout", "10"]);

    let output = ctx.run_debugger_ok(&["backtrace"]);
    assert!(output.contains("main"), "Expected to be in main(): {}", output);

    ctx.run_debugger(&["stop"]);
}

#[test]
#[ignore = "requires js-debug"]
fn test_expression_evaluation_js() {
    let js_debug_path = match js_debug_available() {
        Some(path) => path,
        None => {
            eprintln!("Skipping test: js-debug not available");
            return;
        }
    };

    let node_path = match which::which("node") {
        Ok(path) => path,
        Err(_) => {
            eprintln!("Skipping test: Node.js not available");
            return;
        }
    };

    let ctx = TestContext::new("eval_js");
    ctx.create_config_with_tcp(
        "js-debug",
        node_path.to_str().unwrap(),
        &[js_debug_path.to_str().unwrap()],
        "tcp-port-arg",
    );

    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let js_fixture = PathBuf::from(manifest_dir)
        .join("tests")
        .join("fixtures")
        .join("simple.js");

    let markers = ctx.find_breakpoint_markers(&js_fixture);
    let before_add_line = markers.get("before_add").expect("Missing before_add marker");

    ctx.cleanup_daemon();

    ctx.run_debugger_ok(&["start", js_fixture.to_str().unwrap(), "--stop-on-entry"]);

    let bp_location = format!("simple.js:{}", before_add_line);
    ctx.run_debugger_ok(&["break", &bp_location]);
    ctx.run_debugger_ok(&["continue"]);
    ctx.run_debugger_ok(&["await", "--timeout", "30"]);

    let output = ctx.run_debugger_ok(&["print", "x"]);
    assert!(output.contains("10") || output.contains("x ="), "Expected x=10: {}", output);

    let output = ctx.run_debugger_ok(&["print", "y"]);
    assert!(output.contains("20") || output.contains("y ="), "Expected y=20: {}", output);

    let output = ctx.run_debugger_ok(&["print", "x + y"]);
    assert!(output.contains("30"), "Expected x+y=30: {}", output);

    ctx.run_debugger(&["stop"]);
}
