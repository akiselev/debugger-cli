//! CLI command handling
//!
//! Dispatches CLI commands to the daemon and formats output.

mod spawn;

use crate::commands::{BreakpointCommands, Commands};
use crate::common::{Error, Result};
use crate::ipc::protocol::{
    BreakpointInfo, BreakpointLocation, Command, ContextResult, EvaluateContext, EvaluateResult,
    StackFrameInfo, StatusResult, StopResult, ThreadInfo, VariableInfo,
};
use crate::ipc::DaemonClient;

/// Dispatch a CLI command
pub async fn dispatch(command: Commands) -> Result<()> {
    match command {
        Commands::Daemon => {
            // Should never happen - daemon mode is handled in main
            unreachable!("Daemon command should be handled in main")
        }

        Commands::Start {
            program,
            args,
            adapter,
            stop_on_entry,
        } => {
            spawn::ensure_daemon_running().await?;
            let mut client = DaemonClient::connect().await?;

            let program = program.canonicalize().unwrap_or(program);

            let result = client
                .send_command(Command::Start {
                    program: program.clone(),
                    args,
                    adapter,
                    stop_on_entry,
                })
                .await?;

            println!("Started debugging: {}", program.display());

            if stop_on_entry {
                println!("Stopped at entry point. Use 'debugger continue' to run.");
            } else {
                println!("Program is running. Use 'debugger await' to wait for a stop.");
            }

            Ok(())
        }

        Commands::Attach { pid, adapter } => {
            spawn::ensure_daemon_running().await?;
            let mut client = DaemonClient::connect().await?;

            client.send_command(Command::Attach { pid, adapter }).await?;

            println!("Attached to process {}", pid);
            println!("Program is stopped. Use 'debugger continue' to run.");

            Ok(())
        }

        Commands::Breakpoint(bp_cmd) => match bp_cmd {
            BreakpointCommands::Add {
                location,
                condition,
                hit_count,
            } => {
                let mut client = DaemonClient::connect().await?;
                let loc = BreakpointLocation::parse(&location)?;

                let result = client
                    .send_command(Command::BreakpointAdd {
                        location: loc,
                        condition,
                        hit_count,
                    })
                    .await?;

                let info: BreakpointInfo = serde_json::from_value(result)?;
                print_breakpoint_added(&info);

                Ok(())
            }

            BreakpointCommands::Remove { id, all } => {
                let mut client = DaemonClient::connect().await?;

                client
                    .send_command(Command::BreakpointRemove { id, all })
                    .await?;

                if all {
                    println!("All breakpoints removed");
                } else if let Some(id) = id {
                    println!("Breakpoint {} removed", id);
                }

                Ok(())
            }

            BreakpointCommands::List => {
                let mut client = DaemonClient::connect().await?;

                let result = client.send_command(Command::BreakpointList).await?;
                let breakpoints: Vec<BreakpointInfo> =
                    serde_json::from_value(result["breakpoints"].clone())?;

                if breakpoints.is_empty() {
                    println!("No breakpoints set");
                } else {
                    println!("Breakpoints:");
                    for bp in &breakpoints {
                        print_breakpoint(bp);
                    }
                }

                Ok(())
            }

            BreakpointCommands::Enable { id } => {
                let mut client = DaemonClient::connect().await?;
                client
                    .send_command(Command::BreakpointEnable { id })
                    .await?;
                println!("Breakpoint {} enabled", id);
                Ok(())
            }

            BreakpointCommands::Disable { id } => {
                let mut client = DaemonClient::connect().await?;
                client
                    .send_command(Command::BreakpointDisable { id })
                    .await?;
                println!("Breakpoint {} disabled", id);
                Ok(())
            }
        },

        Commands::Break { location, condition } => {
            // Shorthand for breakpoint add
            let mut client = DaemonClient::connect().await?;
            let loc = BreakpointLocation::parse(&location)?;

            let result = client
                .send_command(Command::BreakpointAdd {
                    location: loc,
                    condition,
                    hit_count: None,
                })
                .await?;

            let info: BreakpointInfo = serde_json::from_value(result)?;
            print_breakpoint_added(&info);

            Ok(())
        }

        Commands::Continue => {
            let mut client = DaemonClient::connect().await?;
            client.send_command(Command::Continue).await?;
            println!("Continuing execution...");
            Ok(())
        }

        Commands::Next => {
            let mut client = DaemonClient::connect().await?;
            client.send_command(Command::Next).await?;
            println!("Stepping over...");
            Ok(())
        }

        Commands::Step => {
            let mut client = DaemonClient::connect().await?;
            client.send_command(Command::StepIn).await?;
            println!("Stepping into...");
            Ok(())
        }

        Commands::Finish => {
            let mut client = DaemonClient::connect().await?;
            client.send_command(Command::StepOut).await?;
            println!("Stepping out...");
            Ok(())
        }

        Commands::Pause => {
            let mut client = DaemonClient::connect().await?;
            client.send_command(Command::Pause).await?;
            println!("Pausing execution...");
            Ok(())
        }

        Commands::Backtrace { limit, locals } => {
            let mut client = DaemonClient::connect().await?;

            let result = client
                .send_command(Command::StackTrace {
                    thread_id: None,
                    limit,
                })
                .await?;

            let frames: Vec<StackFrameInfo> = serde_json::from_value(result["frames"].clone())?;

            if frames.is_empty() {
                println!("No stack frames");
            } else {
                for (i, frame) in frames.iter().enumerate() {
                    let source = frame.source.as_deref().unwrap_or("?");
                    let line = frame.line.map(|l| l.to_string()).unwrap_or_else(|| "?".to_string());
                    println!("#{} {} at {}:{}", i, frame.name, source, line);

                    if locals {
                        // Get locals for this frame
                        let locals_result = client
                            .send_command(Command::Locals {
                                frame_id: Some(frame.id),
                            })
                            .await;

                        if let Ok(result) = locals_result {
                            if let Ok(vars) =
                                serde_json::from_value::<Vec<VariableInfo>>(result["variables"].clone())
                            {
                                for var in vars {
                                    println!(
                                        "    {} = {}{}",
                                        var.name,
                                        var.value,
                                        var.type_name
                                            .map(|t| format!(" ({})", t))
                                            .unwrap_or_default()
                                    );
                                }
                            }
                        }
                    }
                }
            }

            Ok(())
        }

        Commands::Locals => {
            let mut client = DaemonClient::connect().await?;

            let result = client
                .send_command(Command::Locals { frame_id: None })
                .await?;

            let vars: Vec<VariableInfo> = serde_json::from_value(result["variables"].clone())?;

            if vars.is_empty() {
                println!("No local variables");
            } else {
                println!("Local variables:");
                for var in &vars {
                    println!(
                        "  {} = {}{}",
                        var.name,
                        var.value,
                        var.type_name
                            .as_ref()
                            .map(|t| format!(" ({})", t))
                            .unwrap_or_default()
                    );
                }
            }

            Ok(())
        }

        Commands::Print { expression } => {
            let mut client = DaemonClient::connect().await?;

            let result = client
                .send_command(Command::Evaluate {
                    expression: expression.clone(),
                    frame_id: None,
                    context: EvaluateContext::Watch,
                })
                .await?;

            let eval: EvaluateResult = serde_json::from_value(result)?;
            println!(
                "{} = {}{}",
                expression,
                eval.result,
                eval.type_name.map(|t| format!(" ({})", t)).unwrap_or_default()
            );

            Ok(())
        }

        Commands::Eval { expression } => {
            let mut client = DaemonClient::connect().await?;

            let result = client
                .send_command(Command::Evaluate {
                    expression: expression.clone(),
                    frame_id: None,
                    context: EvaluateContext::Repl,
                })
                .await?;

            let eval: EvaluateResult = serde_json::from_value(result)?;
            println!("{}", eval.result);

            Ok(())
        }

        Commands::Context { lines } => {
            let mut client = DaemonClient::connect().await?;

            let result = client.send_command(Command::Context { lines }).await?;

            let ctx: ContextResult = serde_json::from_value(result)?;

            // Print header
            if let Some(source) = &ctx.source {
                println!(
                    "Thread {} stopped at {}:{}",
                    ctx.thread_id, source, ctx.line
                );
            }
            if let Some(func) = &ctx.function {
                println!("In function: {}", func);
            }
            println!();

            // Print source with line numbers
            for line in &ctx.source_lines {
                let marker = if line.is_current { "->" } else { "  " };
                println!("{} {:>4} | {}", marker, line.number, line.content);
            }

            // Print locals
            if !ctx.locals.is_empty() {
                println!();
                println!("Locals:");
                for var in &ctx.locals {
                    println!(
                        "  {} = {}{}",
                        var.name,
                        var.value,
                        var.type_name
                            .as_ref()
                            .map(|t| format!(" ({})", t))
                            .unwrap_or_default()
                    );
                }
            }

            Ok(())
        }

        Commands::Threads => {
            let mut client = DaemonClient::connect().await?;

            let result = client.send_command(Command::Threads).await?;
            let threads: Vec<ThreadInfo> = serde_json::from_value(result["threads"].clone())?;

            if threads.is_empty() {
                println!("No threads");
            } else {
                println!("Threads:");
                for thread in &threads {
                    println!("  {} - {}", thread.id, thread.name);
                }
            }

            Ok(())
        }

        Commands::Thread { id } => {
            let mut client = DaemonClient::connect().await?;

            if let Some(id) = id {
                client
                    .send_command(Command::ThreadSelect { id })
                    .await?;
                println!("Switched to thread {}", id);
            } else {
                // Show current thread info
                let result = client.send_command(Command::Status).await?;
                let status: StatusResult = serde_json::from_value(result)?;
                if let Some(thread_id) = status.stopped_thread {
                    println!("Current thread: {}", thread_id);
                } else {
                    println!("No thread selected");
                }
            }

            Ok(())
        }

        Commands::Frame { number } => {
            let mut client = DaemonClient::connect().await?;

            if let Some(n) = number {
                client
                    .send_command(Command::FrameSelect { number: n })
                    .await?;
                println!("Switched to frame {}", n);
            } else {
                println!("Current frame: 0 (use 'debugger backtrace' to see all frames)");
            }

            Ok(())
        }

        Commands::Up => {
            let mut client = DaemonClient::connect().await?;
            let result = client.send_command(Command::FrameUp).await?;
            print_frame_nav_result(&result);
            Ok(())
        }

        Commands::Down => {
            let mut client = DaemonClient::connect().await?;
            let result = client.send_command(Command::FrameDown).await?;
            print_frame_nav_result(&result);
            Ok(())
        }

        Commands::Await { timeout } => {
            let mut client = DaemonClient::connect().await?;

            println!("Waiting for program to stop (timeout: {}s)...", timeout);

            let result = client
                .send_command(Command::Await {
                    timeout_secs: timeout,
                })
                .await?;

            // Check if we got a stop result or already stopped
            if result.get("already_stopped").and_then(|v| v.as_bool()).unwrap_or(false) {
                let reason = result["reason"].as_str().unwrap_or("unknown");
                println!("Program was already stopped: {}", reason);
            } else if let Some(reason) = result.get("reason").and_then(|v| v.as_str()) {
                match reason {
                    "exited" => {
                        let code = result["exit_code"].as_i64().unwrap_or(0);
                        println!("Program exited with code {}", code);
                    }
                    "terminated" => {
                        println!("Program terminated");
                    }
                    _ => {
                        let stop: StopResult = serde_json::from_value(result)?;
                        print_stop_result(&stop);
                    }
                }
            }

            Ok(())
        }

        Commands::Output { follow, tail, clear } => {
            let mut client = DaemonClient::connect().await?;

            if follow {
                println!("Output streaming not yet implemented");
                return Ok(());
            }

            let result = client
                .send_command(Command::GetOutput { tail, clear })
                .await?;

            let output = result["output"].as_str().unwrap_or("");
            if output.is_empty() {
                println!("(no output)");
            } else {
                print!("{}", output);
            }

            Ok(())
        }

        Commands::Status => {
            match DaemonClient::connect().await {
                Ok(mut client) => {
                    let result = client.send_command(Command::Status).await?;
                    let status: StatusResult = serde_json::from_value(result)?;

                    println!("Daemon: running");
                    if status.session_active {
                        println!("Session: active");
                        if let Some(program) = status.program {
                            println!("Program: {}", program);
                        }
                        if let Some(adapter) = status.adapter {
                            println!("Adapter: {}", adapter);
                        }
                        if let Some(state) = status.state {
                            println!("State: {}", state);
                        }
                        if let Some(reason) = status.stopped_reason {
                            println!("Stopped reason: {}", reason);
                        }
                        if let Some(thread) = status.stopped_thread {
                            println!("Stopped thread: {}", thread);
                        }
                    } else {
                        println!("Session: none");
                    }
                }
                Err(Error::DaemonNotRunning) => {
                    println!("Daemon: not running");
                    println!("Session: none");
                }
                Err(e) => return Err(e),
            }

            Ok(())
        }

        Commands::Stop => {
            let mut client = DaemonClient::connect().await?;
            client.send_command(Command::Stop).await?;
            println!("Debug session stopped");
            Ok(())
        }

        Commands::Detach => {
            let mut client = DaemonClient::connect().await?;
            client.send_command(Command::Detach).await?;
            println!("Detached from process (process continues running)");
            Ok(())
        }

        Commands::Restart => {
            let mut client = DaemonClient::connect().await?;
            client.send_command(Command::Restart).await?;
            println!("Program restarted");
            Ok(())
        }
    }
}

/// Print the result of a frame navigation command (up/down)
fn print_frame_nav_result(result: &serde_json::Value) {
    let frame_index = result["selected"].as_u64().unwrap_or(0);

    if let Ok(frame_info) = serde_json::from_value::<StackFrameInfo>(result["frame"].clone()) {
        let source = frame_info.source.as_deref().unwrap_or("?");
        let line = frame_info
            .line
            .map(|l| l.to_string())
            .unwrap_or_else(|| "?".to_string());
        println!("#{} {} at {}:{}", frame_index, frame_info.name, source, line);
    } else {
        println!("Switched to frame {}", frame_index);
    }
}

fn print_breakpoint_added(info: &BreakpointInfo) {
    if info.verified {
        println!(
            "Breakpoint {} set at {}:{}",
            info.id,
            info.source.as_deref().unwrap_or("?"),
            info.line.map(|l| l.to_string()).unwrap_or_else(|| "?".to_string())
        );
    } else {
        println!(
            "Breakpoint {} pending{}",
            info.id,
            info.message.as_ref().map(|m| format!(": {}", m)).unwrap_or_default()
        );
    }
}

fn print_breakpoint(info: &BreakpointInfo) {
    let status = if info.enabled {
        if info.verified { "✓" } else { "?" }
    } else {
        "○"
    };

    let location = match (&info.source, info.line) {
        (Some(source), Some(line)) => format!("{}:{}", source, line),
        (Some(source), None) => source.clone(),
        (None, Some(line)) => format!(":{}", line),
        (None, None) => "unknown".to_string(),
    };

    let extras = [
        info.condition.as_ref().map(|c| format!("if {}", c)),
        info.hit_count.map(|n| format!("hits: {}", n)),
        info.message.clone(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");

    if extras.is_empty() {
        println!("  {} {} {}", status, info.id, location);
    } else {
        println!("  {} {} {} ({})", status, info.id, location, extras);
    }
}

fn print_stop_result(stop: &StopResult) {
    match stop.reason.as_str() {
        "breakpoint" => {
            println!("Stopped at breakpoint");
            if !stop.hit_breakpoint_ids.is_empty() {
                println!("  Breakpoint IDs: {:?}", stop.hit_breakpoint_ids);
            }
        }
        "step" => {
            println!("Step completed");
        }
        "exception" | "signal" => {
            println!(
                "Stopped: {}",
                stop.description.as_deref().unwrap_or(&stop.reason)
            );
        }
        "pause" => {
            println!("Paused");
        }
        "entry" => {
            println!("Stopped at entry point");
        }
        _ => {
            println!("Stopped: {}", stop.reason);
        }
    }

    if let (Some(source), Some(line)) = (&stop.source, stop.line) {
        println!("  Location: {}:{}", source, line);
    }
}
