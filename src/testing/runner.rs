//! Test runner implementation
//!
//! Executes test scenarios by communicating directly with the daemon
//! using structured data rather than parsing CLI output.

use std::path::Path;
use std::process::Stdio;

use colored::Colorize;
use tokio::process::Command as TokioCommand;

use crate::cli::spawn::ensure_daemon_running;
use crate::common::{Error, Result};
use crate::ipc::protocol::{
    BreakpointLocation, Command, EvaluateContext, EvaluateResult, StackFrameInfo,
    StopResult, VariableInfo,
};
use crate::ipc::DaemonClient;

use super::config::{
    CommandExpectation, EvaluateExpectation, FrameAssertion, StopExpectation, TestScenario,
    TestStep, VariableAssertion,
};

/// Result of a test run
#[derive(Debug)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub steps_run: usize,
    pub steps_total: usize,
    pub error: Option<String>,
}

/// Run a test scenario from a YAML file
pub async fn run_scenario(path: &Path, verbose: bool) -> Result<TestResult> {
    // Load and parse the YAML scenario
    let content = std::fs::read_to_string(path).map_err(|e| {
        Error::Config(format!(
            "Failed to read test scenario '{}': {}",
            path.display(),
            e
        ))
    })?;

    let scenario: TestScenario = serde_yaml::from_str(&content)
        .map_err(|e| Error::Config(format!("Failed to parse test scenario: {}", e)))?;

    let steps_total = scenario.steps.len();

    println!(
        "\n{} {}",
        "Running Test:".blue().bold(),
        scenario.name.white().bold()
    );

    if let Some(desc) = &scenario.description {
        println!("  {}", desc.dimmed());
    }

    // Run setup steps
    if let Some(setup_steps) = &scenario.setup {
        println!("\n{}", "Setup:".cyan());
        for step in setup_steps {
            if verbose {
                println!("  $ {}", step.shell.dimmed());
            }

            let status = TokioCommand::new("sh")
                .arg("-c")
                .arg(&step.shell)
                .stdin(Stdio::null())
                .stdout(if verbose {
                    Stdio::inherit()
                } else {
                    Stdio::null()
                })
                .stderr(if verbose {
                    Stdio::inherit()
                } else {
                    Stdio::null()
                })
                .status()
                .await
                .map_err(|e| Error::Config(format!("Setup command failed to execute: {}", e)))?;

            if !status.success() {
                return Ok(TestResult {
                    name: scenario.name.clone(),
                    passed: false,
                    steps_run: 0,
                    steps_total,
                    error: Some(format!(
                        "Setup command '{}' failed with exit code {:?}",
                        step.shell,
                        status.code()
                    )),
                });
            }
            println!("  {} {}", "✓".green(), step.shell.dimmed());
        }
    }

    // Ensure daemon is running
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect().await?;

    // Resolve program path relative to the scenario file
    let scenario_dir = path.parent().unwrap_or(Path::new("."));
    let program_path = if scenario.target.program.is_relative() {
        scenario_dir.join(&scenario.target.program)
    } else {
        scenario.target.program.clone()
    };

    // Handle launch vs attach mode
    if scenario.target.mode == "attach" {
        // Attach mode: get PID from scenario or pid_file
        let pid = if let Some(pid) = scenario.target.pid {
            pid
        } else if let Some(pid_file_path) = &scenario.target.pid_file {
            let pid_file = if pid_file_path.is_relative() {
                scenario_dir.join(pid_file_path)
            } else {
                pid_file_path.clone()
            };

            let pid_str = std::fs::read_to_string(&pid_file).map_err(|e| {
                Error::Config(format!(
                    "Failed to read PID file '{}': {}",
                    pid_file.display(),
                    e
                ))
            })?;

            pid_str.trim().parse::<u32>().map_err(|e| {
                Error::Config(format!(
                    "Invalid PID in file '{}': {}",
                    pid_file.display(),
                    e
                ))
            })?
        } else {
            return Err(Error::Config(
                "Attach mode requires either 'pid' or 'pid_file' field".to_string(),
            ));
        };

        // Validate process exists before attempting attach (signal 0 checks existence)
        #[cfg(unix)]
        {
            // Signal 0 tests process existence without side effects
            let result = unsafe { libc::kill(pid as i32, 0) };
            if result != 0 {
                return Err(Error::Config(format!(
                    "Process with PID {} not found or not accessible",
                    pid
                )));
            }
        }

        println!("\n{}", "Attaching to process...".cyan());
        client
            .send_command(Command::Attach {
                pid,
                adapter: scenario.target.adapter.clone(),
            })
            .await?;

        if verbose {
            println!("  PID: {}", pid.to_string().dimmed());
            if let Some(adapter) = &scenario.target.adapter {
                println!("  Adapter: {}", adapter.dimmed());
            }
        }

        println!("  {} Attached to process", "✓".green());
    } else if scenario.target.mode != "launch" && scenario.target.mode != "launch" {
        // Unknown mode - fail explicitly
        return Err(Error::Config(format!(
            "Unknown target mode '{}'. Supported modes: 'launch', 'attach'",
            scenario.target.mode
        )));
    } else {
        // Launch mode (default)
        let program_path = program_path.canonicalize().map_err(|e| {
            Error::Config(format!(
                "Program not found '{}': {}",
                scenario.target.program.display(),
                e
            ))
        })?;

        println!("\n{}", "Starting debug session...".cyan());
        client
            .send_command(Command::Start {
                program: program_path.clone(),
                args: scenario.target.args.clone().unwrap_or_default(),
                adapter: scenario.target.adapter.clone(),
                stop_on_entry: scenario.target.stop_on_entry,
                initial_breakpoints: Vec::new(),
            })
            .await?;

        if verbose {
            println!(
                "  Program: {}",
                program_path.display().to_string().dimmed()
            );
            if let Some(adapter) = &scenario.target.adapter {
                println!("  Adapter: {}", adapter.dimmed());
            }
        }

        println!("  {} Session started", "✓".green());
    }

    // Execute test steps
    println!("\n{}", "Steps:".cyan());

    for (i, step) in scenario.steps.iter().enumerate() {
        let step_num = i + 1;

        match execute_step(&mut client, step, step_num, verbose).await {
            Ok(()) => {
                // Step passed
            }
            Err(e) => {
                println!("  {} Step {}: {}", "✗".red(), step_num, e);

                // Cleanup: stop the debug session
                let _ = client.send_command(Command::Stop).await;

                return Ok(TestResult {
                    name: scenario.name.clone(),
                    passed: false,
                    steps_run: step_num,
                    steps_total,
                    error: Some(e.to_string()),
                });
            }
        }
    }

    // Cleanup: stop the debug session
    let _ = client.send_command(Command::Stop).await;

    println!(
        "\n{} {}\n",
        "✓".green().bold(),
        "Test Passed".green().bold()
    );

    Ok(TestResult {
        name: scenario.name,
        passed: true,
        steps_run: steps_total,
        steps_total,
        error: None,
    })
}

/// Execute a single test step
async fn execute_step(
    client: &mut DaemonClient,
    step: &TestStep,
    step_num: usize,
    verbose: bool,
) -> Result<()> {
    match step {
        TestStep::Command { command, expect } => {
            execute_command_step(client, command, expect.as_ref(), step_num, verbose).await
        }
        TestStep::Await { timeout, expect } => {
            execute_await_step(client, *timeout, expect.as_ref(), step_num, verbose).await
        }
        TestStep::InspectLocals { asserts } => {
            execute_inspect_locals_step(client, asserts, step_num, verbose).await
        }
        TestStep::InspectStack { asserts } => {
            execute_inspect_stack_step(client, asserts, step_num, verbose).await
        }
        TestStep::CheckOutput { contains, equals } => {
            execute_check_output_step(client, contains.as_ref(), equals.as_ref(), step_num, verbose)
                .await
        }
        TestStep::Evaluate { expression, expect } => {
            execute_evaluate_step(client, expression, expect.as_ref(), step_num, verbose).await
        }
    }
}

/// Execute a command step
async fn execute_command_step(
    client: &mut DaemonClient,
    command_str: &str,
    expect: Option<&CommandExpectation>,
    step_num: usize,
    _verbose: bool,
) -> Result<()> {
    let cmd = parse_command(command_str)?;

    let result = client.send_command(cmd).await;

    // Check expectations
    if let Some(exp) = expect {
        if let Some(should_succeed) = exp.success {
            let did_succeed = result.is_ok();
            if should_succeed != did_succeed {
                return Err(Error::TestAssertion(format!(
                    "Command '{}' expected success={}, got success={}",
                    command_str, should_succeed, did_succeed
                )));
            }
        }
    }

    // For commands that are expected to fail, we don't propagate the error
    if expect.map(|e| e.success == Some(false)).unwrap_or(false) {
        println!(
            "  {} Step {}: {} (expected failure)",
            "✓".green(),
            step_num,
            command_str.dimmed()
        );
        return Ok(());
    }

    result?;

    println!(
        "  {} Step {}: {}",
        "✓".green(),
        step_num,
        command_str.dimmed()
    );

    Ok(())
}

/// Execute an await step
async fn execute_await_step(
    client: &mut DaemonClient,
    timeout: Option<u64>,
    expect: Option<&StopExpectation>,
    step_num: usize,
    _verbose: bool,
) -> Result<()> {
    let timeout_secs = timeout.unwrap_or(30);

    let result = client
        .send_command(Command::Await { timeout_secs })
        .await?;

    let stop_result: StopResult = serde_json::from_value(result)
        .map_err(|e| Error::TestAssertion(format!("Failed to parse stop result: {}", e)))?;

    // Check expectations
    if let Some(exp) = expect {
        if let Some(expected_reason) = &exp.reason {
            if !stop_result.reason.contains(expected_reason) {
                return Err(Error::TestAssertion(format!(
                    "Expected stop reason '{}', got '{}'",
                    expected_reason, stop_result.reason
                )));
            }
        }

        if let Some(expected_file) = &exp.file {
            let actual_file = stop_result.source.as_deref().unwrap_or("");
            if !actual_file.contains(expected_file) {
                return Err(Error::TestAssertion(format!(
                    "Expected file '{}', got '{}'",
                    expected_file, actual_file
                )));
            }
        }

        if let Some(expected_line) = exp.line {
            let actual_line = stop_result.line.unwrap_or(0);
            if expected_line != actual_line {
                return Err(Error::TestAssertion(format!(
                    "Expected line {}, got {}",
                    expected_line, actual_line
                )));
            }
        }
    }

    let location = if let Some(source) = &stop_result.source {
        if let Some(line) = stop_result.line {
            format!("{}:{}", source, line)
        } else {
            source.clone()
        }
    } else {
        "unknown location".to_string()
    };

    println!(
        "  {} Step {}: await ({} at {})",
        "✓".green(),
        step_num,
        stop_result.reason.dimmed(),
        location.dimmed()
    );

    Ok(())
}

/// Execute an inspect locals step
async fn execute_inspect_locals_step(
    client: &mut DaemonClient,
    asserts: &[VariableAssertion],
    step_num: usize,
    _verbose: bool,
) -> Result<()> {
    let result = client
        .send_command(Command::Locals { frame_id: None })
        .await?;

    let vars: Vec<VariableInfo> = serde_json::from_value(result["variables"].clone())
        .map_err(|e| Error::TestAssertion(format!("Failed to parse variables: {}", e)))?;

    for assertion in asserts {
        let var = vars.iter().find(|v| v.name == assertion.name);

        match var {
            Some(v) => {
                // Check value (exact match)
                if let Some(expected_value) = &assertion.value {
                    if &v.value != expected_value {
                        return Err(Error::TestAssertion(format!(
                            "Variable '{}': expected value '{}', got '{}'",
                            assertion.name, expected_value, v.value
                        )));
                    }
                }

                // Check value (partial match)
                if let Some(expected_substr) = &assertion.value_contains {
                    if !v.value.contains(expected_substr) {
                        return Err(Error::TestAssertion(format!(
                            "Variable '{}': expected value containing '{}', got '{}'",
                            assertion.name, expected_substr, v.value
                        )));
                    }
                }

                // Check type
                if let Some(expected_type) = &assertion.type_name {
                    let actual_type = v.type_name.as_deref().unwrap_or("");
                    if actual_type != expected_type {
                        return Err(Error::TestAssertion(format!(
                            "Variable '{}': expected type '{}', got '{}'",
                            assertion.name, expected_type, actual_type
                        )));
                    }
                }
            }
            None => {
                let available: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
                return Err(Error::TestAssertion(format!(
                    "Variable '{}' not found. Available: {:?}",
                    assertion.name, available
                )));
            }
        }
    }

    let checked: Vec<&str> = asserts.iter().map(|a| a.name.as_str()).collect();
    println!(
        "  {} Step {}: inspect locals ({:?})",
        "✓".green(),
        step_num,
        checked
    );

    Ok(())
}

/// Execute an inspect stack step
async fn execute_inspect_stack_step(
    client: &mut DaemonClient,
    asserts: &[FrameAssertion],
    step_num: usize,
    _verbose: bool,
) -> Result<()> {
    let result = client
        .send_command(Command::StackTrace {
            thread_id: None,
            limit: 50,
        })
        .await?;

    let frames: Vec<StackFrameInfo> = serde_json::from_value(result["frames"].clone())
        .map_err(|e| Error::TestAssertion(format!("Failed to parse stack frames: {}", e)))?;

    for assertion in asserts {
        if assertion.index >= frames.len() {
            return Err(Error::TestAssertion(format!(
                "Frame {} does not exist (only {} frames)",
                assertion.index,
                frames.len()
            )));
        }

        let frame = &frames[assertion.index];

        if let Some(expected_func) = &assertion.function {
            if !frame.name.contains(expected_func) {
                return Err(Error::TestAssertion(format!(
                    "Frame {}: expected function '{}', got '{}'",
                    assertion.index, expected_func, frame.name
                )));
            }
        }

        if let Some(expected_file) = &assertion.file {
            let actual_file = frame.source.as_deref().unwrap_or("");
            if !actual_file.contains(expected_file) {
                return Err(Error::TestAssertion(format!(
                    "Frame {}: expected file '{}', got '{}'",
                    assertion.index, expected_file, actual_file
                )));
            }
        }

        if let Some(expected_line) = assertion.line {
            let actual_line = frame.line.unwrap_or(0);
            if expected_line != actual_line {
                return Err(Error::TestAssertion(format!(
                    "Frame {}: expected line {}, got {}",
                    assertion.index, expected_line, actual_line
                )));
            }
        }
    }

    println!(
        "  {} Step {}: inspect stack ({} frames checked)",
        "✓".green(),
        step_num,
        asserts.len()
    );

    Ok(())
}

/// Execute a check output step
async fn execute_check_output_step(
    client: &mut DaemonClient,
    contains: Option<&String>,
    equals: Option<&String>,
    step_num: usize,
    _verbose: bool,
) -> Result<()> {
    let result = client
        .send_command(Command::GetOutput {
            tail: None,
            clear: false,
        })
        .await?;

    let output = result["output"].as_str().unwrap_or("");

    if let Some(expected_substr) = contains {
        if !output.contains(expected_substr) {
            return Err(Error::TestAssertion(format!(
                "Output does not contain '{}'. Got: '{}'",
                expected_substr,
                if output.len() > 200 {
                    format!("{}...", &output[..200])
                } else {
                    output.to_string()
                }
            )));
        }
    }

    if let Some(expected_exact) = equals {
        if output.trim() != expected_exact.trim() {
            return Err(Error::TestAssertion(format!(
                "Output mismatch. Expected: '{}', got: '{}'",
                expected_exact, output
            )));
        }
    }

    println!(
        "  {} Step {}: check output",
        "✓".green(),
        step_num
    );

    Ok(())
}

/// Execute an evaluate step
async fn execute_evaluate_step(
    client: &mut DaemonClient,
    expression: &str,
    expect: Option<&EvaluateExpectation>,
    step_num: usize,
    _verbose: bool,
) -> Result<()> {
    let result = client
        .send_command(Command::Evaluate {
            expression: expression.to_string(),
            frame_id: None,
            context: EvaluateContext::Watch,
        })
        .await;

    // Check if we expect failure
    let expect_success = expect.and_then(|e| e.success).unwrap_or(true);

    if !expect_success {
        // We expect evaluation to fail
        match result {
            Err(_) => {
                println!(
                    "  {} Step {}: evaluate '{}' (expected failure)",
                    "✓".green(),
                    step_num,
                    expression.dimmed()
                );
                return Ok(());
            }
            Ok(val) => {
                // Check if the result contains an error indicator
                let eval_result: EvaluateResult = serde_json::from_value(val)
                    .map_err(|e| Error::TestAssertion(format!("Failed to parse evaluate result: {}", e)))?;

                // If result_contains is specified, check if error message matches
                if let Some(exp) = expect {
                    if let Some(expected_substr) = &exp.result_contains {
                        if eval_result.result.to_lowercase().contains(&expected_substr.to_lowercase()) {
                            println!(
                                "  {} Step {}: evaluate '{}' = {} (expected error)",
                                "✓".green(),
                                step_num,
                                expression.dimmed(),
                                eval_result.result.dimmed()
                            );
                            return Ok(());
                        }
                    }
                }

                return Err(Error::TestAssertion(format!(
                    "Evaluate '{}': expected failure but got result '{}'",
                    expression, eval_result.result
                )));
            }
        }
    }

    // Normal success path
    let result = result?;
    let eval_result: EvaluateResult = serde_json::from_value(result)
        .map_err(|e| Error::TestAssertion(format!("Failed to parse evaluate result: {}", e)))?;

    if let Some(exp) = expect {
        if let Some(expected_result) = &exp.result {
            if &eval_result.result != expected_result {
                return Err(Error::TestAssertion(format!(
                    "Evaluate '{}': expected '{}', got '{}'",
                    expression, expected_result, eval_result.result
                )));
            }
        }

        if let Some(expected_substr) = &exp.result_contains {
            if !eval_result.result.contains(expected_substr) {
                return Err(Error::TestAssertion(format!(
                    "Evaluate '{}': expected result containing '{}', got '{}'",
                    expression, expected_substr, eval_result.result
                )));
            }
        }

        if let Some(expected_type) = &exp.type_name {
            let actual_type = eval_result.type_name.as_deref().unwrap_or("");
            if actual_type != expected_type {
                return Err(Error::TestAssertion(format!(
                    "Evaluate '{}': expected type '{}', got '{}'",
                    expression, expected_type, actual_type
                )));
            }
        }
    }

    println!(
        "  {} Step {}: evaluate '{}' = {}",
        "✓".green(),
        step_num,
        expression.dimmed(),
        eval_result.result.dimmed()
    );

    Ok(())
}

/// Parse a command string into a Command enum
fn parse_command(s: &str) -> Result<Command> {
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.is_empty() {
        return Err(Error::Config("Empty command".to_string()));
    }

    let cmd = parts[0].to_lowercase();
    let args = &parts[1..];

    match cmd.as_str() {
        "continue" | "c" => Ok(Command::Continue),
        "next" | "n" => Ok(Command::Next),
        "step" | "s" => Ok(Command::StepIn),
        "finish" | "out" => Ok(Command::StepOut),
        "pause" => Ok(Command::Pause),

        "break" | "b" => {
            if args.is_empty() {
                return Err(Error::Config(
                    "break command requires a location".to_string(),
                ));
            }
            // Handle "break add <location>" or just "break <location>"
            // Also handle --condition "expr" and --hit-count N flags
            let mut location_str = String::new();
            let mut condition: Option<String> = None;
            let mut hit_count: Option<u32> = None;
            let mut i = 0;

            // Skip "add" subcommand if present AND there are more args
            // (otherwise "add" is the function name to break on)
            if args.get(0) == Some(&"add") && args.len() > 1 {
                i = 1;
            }

            while i < args.len() {
                if args[i] == "--condition" && i + 1 < args.len() {
                    // Collect condition expression (may be quoted)
                    i += 1;
                    let mut cond_parts = Vec::new();
                    while i < args.len() && !args[i].starts_with("--") {
                        cond_parts.push(args[i]);
                        i += 1;
                    }
                    condition = Some(cond_parts.join(" ").trim_matches('"').to_string());
                } else if args[i] == "--hit-count" && i + 1 < args.len() {
                    i += 1;
                    hit_count = Some(args[i].parse().map_err(|_| {
                        Error::Config(format!("Invalid hit count: {}", args[i]))
                    })?);
                    i += 1;
                } else if !args[i].starts_with("--") {
                    if !location_str.is_empty() {
                        location_str.push(' ');
                    }
                    location_str.push_str(args[i]);
                    i += 1;
                } else {
                    i += 1;
                }
            }

            let location = BreakpointLocation::parse(&location_str)?;
            Ok(Command::BreakpointAdd {
                location,
                condition,
                hit_count,
            })
        }

        "breakpoint" => {
            if args.is_empty() {
                return Err(Error::Config(
                    "breakpoint command requires a subcommand".to_string(),
                ));
            }

            match args[0] {
                "add" => {
                    if args.len() < 2 {
                        return Err(Error::Config(
                            "breakpoint add requires a location".to_string(),
                        ));
                    }
                    let location = BreakpointLocation::parse(args[1])?;
                    Ok(Command::BreakpointAdd {
                        location,
                        condition: None,
                        hit_count: None,
                    })
                }
                "remove" => {
                    if args.len() < 2 {
                        return Ok(Command::BreakpointRemove { id: None, all: true });
                    }
                    if args[1] == "all" || args[1] == "--all" {
                        return Ok(Command::BreakpointRemove { id: None, all: true });
                    }
                    let id: u32 = args[1].parse().map_err(|_| {
                        Error::Config(format!("Invalid breakpoint ID: {}", args[1]))
                    })?;
                    Ok(Command::BreakpointRemove {
                        id: Some(id),
                        all: false,
                    })
                }
                "list" => Ok(Command::BreakpointList),
                "enable" => {
                    if args.len() < 2 {
                        return Err(Error::Config(
                            "breakpoint enable requires an ID".to_string(),
                        ));
                    }
                    let id: u32 = args[1].parse().map_err(|_| {
                        Error::Config(format!("Invalid breakpoint ID: {}", args[1]))
                    })?;
                    Ok(Command::BreakpointEnable { id })
                }
                "disable" => {
                    if args.len() < 2 {
                        return Err(Error::Config(
                            "breakpoint disable requires an ID".to_string(),
                        ));
                    }
                    let id: u32 = args[1].parse().map_err(|_| {
                        Error::Config(format!("Invalid breakpoint ID: {}", args[1]))
                    })?;
                    Ok(Command::BreakpointDisable { id })
                }
                _ => Err(Error::Config(format!(
                    "Unknown breakpoint subcommand: {}",
                    args[0]
                ))),
            }
        }

        "locals" => Ok(Command::Locals { frame_id: None }),

        "backtrace" | "bt" => Ok(Command::StackTrace {
            thread_id: None,
            limit: 20,
        }),

        "threads" => Ok(Command::Threads),

        "thread" => {
            if args.is_empty() {
                return Err(Error::Config("thread command requires an ID".to_string()));
            }
            let id: i64 = args[0]
                .parse()
                .map_err(|_| Error::Config(format!("Invalid thread ID: {}", args[0])))?;
            Ok(Command::ThreadSelect { id })
        }

        "frame" => {
            if args.is_empty() {
                return Err(Error::Config(
                    "frame command requires a number".to_string(),
                ));
            }
            let number: usize = args[0]
                .parse()
                .map_err(|_| Error::Config(format!("Invalid frame number: {}", args[0])))?;
            Ok(Command::FrameSelect { number })
        }

        "up" => Ok(Command::FrameUp),
        "down" => Ok(Command::FrameDown),

        "print" | "p" | "eval" => {
            if args.is_empty() {
                return Err(Error::Config(
                    "print/eval command requires an expression".to_string(),
                ));
            }
            Ok(Command::Evaluate {
                expression: args.join(" "),
                frame_id: None,
                context: EvaluateContext::Watch,
            })
        }

        "stop" => Ok(Command::Stop),
        "detach" => Ok(Command::Detach),
        "restart" => Ok(Command::Restart),

        "output" => {
            // Parse --tail N and --clear flags
            let mut tail: Option<usize> = None;
            let mut clear = false;
            let mut i = 0;
            while i < args.len() {
                match args[i] {
                    "--tail" => {
                        if i + 1 < args.len() {
                            tail = args[i + 1].parse().ok();
                            i += 2;
                        } else {
                            i += 1;
                        }
                    }
                    "--clear" => {
                        clear = true;
                        i += 1;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            Ok(Command::GetOutput { tail, clear })
        }

        _ => Err(Error::Config(format!("Unknown command: {}", cmd))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_commands() {
        assert!(matches!(parse_command("continue").unwrap(), Command::Continue));
        assert!(matches!(parse_command("c").unwrap(), Command::Continue));
        assert!(matches!(parse_command("next").unwrap(), Command::Next));
        assert!(matches!(parse_command("step").unwrap(), Command::StepIn));
        assert!(matches!(parse_command("finish").unwrap(), Command::StepOut));
        assert!(matches!(parse_command("pause").unwrap(), Command::Pause));
    }

    #[test]
    fn test_parse_break_commands() {
        let cmd = parse_command("break main").unwrap();
        assert!(matches!(cmd, Command::BreakpointAdd { .. }));

        let cmd = parse_command("break add main.rs:42").unwrap();
        assert!(matches!(cmd, Command::BreakpointAdd { .. }));

        let cmd = parse_command("b foo.c:10").unwrap();
        assert!(matches!(cmd, Command::BreakpointAdd { .. }));
    }

    #[test]
    fn test_parse_breakpoint_subcommands() {
        assert!(matches!(
            parse_command("breakpoint add main").unwrap(),
            Command::BreakpointAdd { .. }
        ));
        assert!(matches!(
            parse_command("breakpoint list").unwrap(),
            Command::BreakpointList
        ));
        assert!(matches!(
            parse_command("breakpoint remove 1").unwrap(),
            Command::BreakpointRemove { .. }
        ));
    }

    #[test]
    fn test_parse_print_commands() {
        let cmd = parse_command("print x + y").unwrap();
        match cmd {
            Command::Evaluate { expression, .. } => {
                assert_eq!(expression, "x + y");
            }
            _ => panic!("Expected Evaluate command"),
        }
    }

    #[test]
    fn test_parse_break_with_hit_count() {
        let cmd = parse_command("break factorial --hit-count 3").unwrap();
        match cmd {
            Command::BreakpointAdd { hit_count, .. } => {
                assert_eq!(hit_count, Some(3));
            }
            _ => panic!("Expected BreakpointAdd command"),
        }

        let cmd = parse_command("break main.c:10 --hit-count 5").unwrap();
        match cmd {
            Command::BreakpointAdd { hit_count, .. } => {
                assert_eq!(hit_count, Some(5));
            }
            _ => panic!("Expected BreakpointAdd command"),
        }
    }

    #[test]
    fn test_parse_break_with_condition_and_hit_count() {
        let cmd = parse_command("break foo --condition \"x > 5\" --hit-count 2").unwrap();
        match cmd {
            Command::BreakpointAdd { condition, hit_count, .. } => {
                assert_eq!(condition, Some("x > 5".to_string()));
                assert_eq!(hit_count, Some(2));
            }
            _ => panic!("Expected BreakpointAdd command"),
        }
    }
}
