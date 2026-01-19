//! CLI command handling
//!
//! Dispatches CLI commands to the daemon and formats output.

mod spawn;

use crate::commands::{BreakpointCommands, Commands, WatchCommands};
use crate::common::{Error, Result};
use crate::ipc::protocol::{
    BreakpointInfo, BreakpointLocation, Command, ContextResult, EvaluateContext, EvaluateResult,
    StackFrameInfo, StatusResult, StopResult, ThreadInfo, VariableInfo, WatchpointInfo,
};
use crate::ipc::DaemonClient;
use crate::setup;

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

        Commands::Setup {
            debugger,
            version,
            list,
            check,
            auto_detect,
            uninstall,
            path,
            force,
            dry_run,
            json,
        } => {
            let opts = setup::SetupOptions {
                debugger,
                version,
                list,
                check,
                auto_detect,
                uninstall,
                path,
                force,
                dry_run,
                json,
            };
            setup::run(opts).await
        }

        Commands::Set { name, value } => {
            let mut client = DaemonClient::connect().await?;

            let result = client
                .send_command(Command::SetVariable {
                    name: name.clone(),
                    value: value.clone(),
                    variables_reference: None,
                })
                .await?;

            let new_value = result["value"].as_str().unwrap_or(&value);
            let type_name = result["type"].as_str();

            if let Some(t) = type_name {
                println!("{} = {} ({})", name, new_value, t);
            } else {
                println!("{} = {}", name, new_value);
            }

            Ok(())
        }

        Commands::Memory { address, count, format } => {
            let mut client = DaemonClient::connect().await?;

            let result = client
                .send_command(Command::ReadMemory { address: address.clone(), count })
                .await?;

            let addr = result["address"].as_str().unwrap_or(&address);
            let data: Vec<u8> = serde_json::from_value(result["data"].clone()).unwrap_or_default();

            if data.is_empty() {
                println!("No data read from {}", addr);
                return Ok(());
            }

            println!("Memory at {}:", addr);
            print_memory(&data, &format);

            Ok(())
        }

        Commands::Disassemble { address, count } => {
            let mut client = DaemonClient::connect().await?;

            let result = client
                .send_command(Command::Disassemble { address, count })
                .await?;

            let instructions: Vec<serde_json::Value> =
                serde_json::from_value(result["instructions"].clone()).unwrap_or_default();

            if instructions.is_empty() {
                println!("No instructions to display");
            } else {
                for inst in &instructions {
                    let addr = inst["address"].as_str().unwrap_or("?");
                    let instr = inst["instruction"].as_str().unwrap_or("?");
                    let symbol = inst["symbol"].as_str();
                    let source = inst["source_file"].as_str();
                    let line = inst["line"].as_u64();

                    // Print symbol if present
                    if let Some(sym) = symbol {
                        println!("{}:", sym);
                    }

                    // Print instruction
                    print!("  {:16}  {}", addr, instr);

                    // Print source info if present
                    if let (Some(src), Some(l)) = (source, line) {
                        print!("  ; {}:{}", src, l);
                    }
                    println!();
                }
            }

            Ok(())
        }

        Commands::Watch(watch_cmd) => match watch_cmd {
            WatchCommands::Add {
                variable,
                access,
                condition,
            } => {
                let mut client = DaemonClient::connect().await?;

                let result = client
                    .send_command(Command::WatchpointAdd {
                        variable: variable.clone(),
                        access_type: access,
                        condition,
                    })
                    .await?;

                let info: WatchpointInfo = serde_json::from_value(result)?;
                print_watchpoint_added(&info);

                Ok(())
            }

            WatchCommands::Remove { id } => {
                let mut client = DaemonClient::connect().await?;

                client
                    .send_command(Command::WatchpointRemove { id })
                    .await?;

                println!("Watchpoint {} removed", id);

                Ok(())
            }

            WatchCommands::List => {
                let mut client = DaemonClient::connect().await?;

                let result = client.send_command(Command::WatchpointList).await?;
                let watchpoints: Vec<WatchpointInfo> =
                    serde_json::from_value(result["watchpoints"].clone())?;

                if watchpoints.is_empty() {
                    println!("No watchpoints set");
                } else {
                    println!("Watchpoints:");
                    for wp in &watchpoints {
                        print_watchpoint(wp);
                    }
                }

                Ok(())
            }
        },
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
        "data breakpoint" | "watchpoint" => {
            println!("Stopped: watchpoint hit");
        }
        _ => {
            println!("Stopped: {}", stop.reason);
        }
    }

    if let (Some(source), Some(line)) = (&stop.source, stop.line) {
        println!("  Location: {}:{}", source, line);
    }
}

fn print_watchpoint_added(info: &WatchpointInfo) {
    if info.verified {
        println!(
            "Watchpoint {} set on '{}' ({})",
            info.id, info.variable, info.access_type
        );
    } else {
        println!(
            "Watchpoint {} pending on '{}'{}",
            info.id,
            info.variable,
            info.message.as_ref().map(|m| format!(": {}", m)).unwrap_or_default()
        );
    }
}

fn print_watchpoint(info: &WatchpointInfo) {
    let status = if info.enabled {
        if info.verified { "✓" } else { "?" }
    } else {
        "○"
    };

    let extras = info.message.clone();

    if let Some(msg) = extras {
        println!("  {} {} '{}' ({}) - {}", status, info.id, info.variable, info.access_type, msg);
    } else {
        println!("  {} {} '{}' ({})", status, info.id, info.variable, info.access_type);
    }
}

fn print_memory(data: &[u8], format: &str) {
    match format.to_lowercase().as_str() {
        "hex" | "h" => {
            // Print in hexdump format
            for (i, chunk) in data.chunks(16).enumerate() {
                let offset = i * 16;
                print!("{:08x}  ", offset);

                // Hex bytes
                for (j, byte) in chunk.iter().enumerate() {
                    print!("{:02x} ", byte);
                    if j == 7 {
                        print!(" ");
                    }
                }

                // Padding for incomplete lines
                for j in chunk.len()..16 {
                    print!("   ");
                    if j == 7 {
                        print!(" ");
                    }
                }

                // ASCII representation
                print!(" |");
                for byte in chunk {
                    if *byte >= 0x20 && *byte < 0x7f {
                        print!("{}", *byte as char);
                    } else {
                        print!(".");
                    }
                }
                println!("|");
            }
        }
        "decimal" | "d" => {
            for (i, byte) in data.iter().enumerate() {
                if i > 0 && i % 16 == 0 {
                    println!();
                }
                print!("{:3} ", byte);
            }
            println!();
        }
        "ascii" | "a" => {
            for byte in data {
                if *byte >= 0x20 && *byte < 0x7f {
                    print!("{}", *byte as char);
                } else {
                    print!(".");
                }
            }
            println!();
        }
        _ => {
            // Default to hex
            print_memory(data, "hex");
        }
    }
}
