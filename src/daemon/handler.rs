//! Command handler for processing IPC requests
//!
//! Translates IPC commands into session operations and DAP requests.

use serde_json::json;

use crate::common::{config::Config, error::IpcError, Error, Result};
use crate::dap::Event;
use crate::ipc::protocol::{
    BreakpointLocation, Command, ContextResult, EvaluateContext, EvaluateResult, Response,
    SourceLine, StackFrameInfo, StatusResult, StopResult, ThreadInfo, VariableInfo,
};

use super::session::{DebugSession, SessionState};

/// Handle an IPC command
pub async fn handle_command(
    session: &mut Option<DebugSession>,
    config: &Config,
    id: u64,
    command: Command,
) -> Response {
    match handle_command_inner(session, config, command).await {
        Ok(result) => Response::success(id, result),
        Err(e) => Response::error(id, IpcError::from(&e)),
    }
}

async fn handle_command_inner(
    session: &mut Option<DebugSession>,
    config: &Config,
    command: Command,
) -> Result<serde_json::Value> {
    match command {
        // === Session Management ===
        Command::Start {
            program,
            args,
            adapter,
            stop_on_entry,
        } => {
            if session.is_some() {
                return Err(Error::SessionAlreadyActive);
            }

            let new_session =
                DebugSession::launch(config, &program, args, adapter, stop_on_entry).await?;
            *session = Some(new_session);

            Ok(json!({
                "status": "started",
                "program": program.display().to_string()
            }))
        }

        Command::Attach { pid, adapter } => {
            if session.is_some() {
                return Err(Error::SessionAlreadyActive);
            }

            let new_session = DebugSession::attach(config, pid, adapter).await?;
            *session = Some(new_session);

            Ok(json!({
                "status": "attached",
                "pid": pid
            }))
        }

        Command::Detach => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.detach().await?;
            *session = None;

            Ok(json!({ "status": "detached" }))
        }

        Command::Stop => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.stop().await?;
            *session = None;

            Ok(json!({ "status": "stopped" }))
        }

        Command::Restart => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;

            // Check if adapter supports restart
            if sess.capabilities().supports_restart_request {
                sess.restart().await?;
                Ok(json!({ "status": "restarted" }))
            } else {
                // Return helpful error message
                Err(Error::Internal(
                    "Debug adapter does not support restart. Use 'debugger stop' then 'debugger start' manually.".to_string()
                ))
            }
        }

        Command::Status => {
            let result = if let Some(sess) = session {
                StatusResult {
                    daemon_running: true,
                    session_active: true,
                    state: Some(sess.state().to_string()),
                    program: Some(sess.program().display().to_string()),
                    adapter: Some(sess.adapter_name().to_string()),
                    stopped_thread: sess.stopped_thread(),
                    stopped_reason: sess.stopped_reason().map(String::from),
                }
            } else {
                StatusResult {
                    daemon_running: true,
                    session_active: false,
                    state: None,
                    program: None,
                    adapter: None,
                    stopped_thread: None,
                    stopped_reason: None,
                }
            };

            Ok(serde_json::to_value(result)?)
        }

        // === Breakpoints ===
        Command::BreakpointAdd {
            location,
            condition,
            hit_count,
        } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;

            // Check capabilities before using advanced features
            if matches!(location, BreakpointLocation::Function { .. }) {
                if !sess.supports_function_breakpoints() {
                    return Err(Error::Internal(
                        "Debug adapter does not support function breakpoints. Use file:line format instead.".to_string()
                    ));
                }
            }

            if condition.is_some() && !sess.supports_conditional_breakpoints() {
                return Err(Error::Internal(
                    "Debug adapter does not support conditional breakpoints.".to_string()
                ));
            }

            if hit_count.is_some() && !sess.supports_hit_conditional_breakpoints() {
                return Err(Error::Internal(
                    "Debug adapter does not support hit count conditions.".to_string()
                ));
            }

            let info = sess.add_breakpoint(location, condition, hit_count).await?;
            Ok(serde_json::to_value(info)?)
        }

        Command::BreakpointRemove { id, all } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;

            if all {
                sess.remove_all_breakpoints().await?;
                Ok(json!({ "removed": "all" }))
            } else if let Some(id) = id {
                sess.remove_breakpoint(id).await?;
                Ok(json!({ "removed": id }))
            } else {
                Err(Error::InvalidLocation(
                    "Must specify breakpoint ID or --all".to_string(),
                ))
            }
        }

        Command::BreakpointList => {
            let sess = session.as_ref().ok_or(Error::SessionNotActive)?;
            let breakpoints = sess.list_breakpoints();
            Ok(json!({ "breakpoints": breakpoints }))
        }

        Command::BreakpointEnable { id } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.enable_breakpoint(id).await?;
            Ok(json!({ "enabled": id }))
        }

        Command::BreakpointDisable { id } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.disable_breakpoint(id).await?;
            Ok(json!({ "disabled": id }))
        }

        // === Execution Control ===
        Command::Continue => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.continue_execution().await?;
            Ok(json!({ "status": "running" }))
        }

        Command::Next => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.next().await?;
            Ok(json!({ "status": "stepping" }))
        }

        Command::StepIn => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.step_in().await?;
            Ok(json!({ "status": "stepping" }))
        }

        Command::StepOut => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.step_out().await?;
            Ok(json!({ "status": "stepping" }))
        }

        Command::Pause => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.pause().await?;
            Ok(json!({ "status": "pausing" }))
        }

        // === State Inspection ===
        Command::StackTrace { thread_id, limit } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let frames = sess.stack_trace(limit).await?;

            let frame_infos: Vec<StackFrameInfo> = frames
                .iter()
                .map(|f| StackFrameInfo {
                    id: f.id,
                    name: f.name.clone(),
                    source: f.source.as_ref().and_then(|s| s.path.clone()),
                    line: Some(f.line),
                    column: Some(f.column),
                })
                .collect();

            Ok(json!({ "frames": frame_infos }))
        }

        Command::Locals { frame_id } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let vars = sess.get_locals(frame_id).await?;

            let var_infos: Vec<VariableInfo> = vars
                .iter()
                .map(|v| VariableInfo {
                    name: v.name.clone(),
                    value: v.value.clone(),
                    type_name: v.type_name.clone(),
                    variables_reference: v.variables_reference,
                })
                .collect();

            Ok(json!({ "variables": var_infos }))
        }

        Command::Evaluate {
            expression,
            frame_id,
            context,
        } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let ctx_str = match context {
                EvaluateContext::Watch => "watch",
                EvaluateContext::Repl => "repl",
                EvaluateContext::Hover => "hover",
            };
            let result = sess.evaluate(&expression, frame_id, ctx_str).await?;

            Ok(serde_json::to_value(EvaluateResult {
                result: result.result,
                type_name: result.type_name,
                variables_reference: result.variables_reference,
            })?)
        }

        Command::Scopes { frame_id } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let scopes = sess.get_scopes(Some(frame_id)).await?;
            Ok(json!({ "scopes": scopes }))
        }

        Command::Variables { reference } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let vars = sess.get_variables(reference).await?;

            let var_infos: Vec<VariableInfo> = vars
                .iter()
                .map(|v| VariableInfo {
                    name: v.name.clone(),
                    value: v.value.clone(),
                    type_name: v.type_name.clone(),
                    variables_reference: v.variables_reference,
                })
                .collect();

            Ok(json!({ "variables": var_infos }))
        }

        // === Thread/Frame Management ===
        Command::Threads => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let threads = sess.get_threads().await?;

            let thread_infos: Vec<ThreadInfo> = threads
                .iter()
                .map(|t| ThreadInfo {
                    id: t.id,
                    name: t.name.clone(),
                    state: None, // DAP doesn't provide this directly
                })
                .collect();

            Ok(json!({ "threads": thread_infos }))
        }

        Command::ThreadSelect { id } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            sess.select_thread(id)?;
            Ok(json!({ "selected": id }))
        }

        Command::FrameSelect { number } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let frame = sess.select_frame(number).await?;
            Ok(create_frame_response(&frame, number))
        }

        Command::FrameUp => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let frame = sess.frame_up().await?;
            let index = sess.get_current_frame_index();
            Ok(create_frame_response(&frame, index))
        }

        Command::FrameDown => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let frame = sess.frame_down().await?;
            let index = sess.get_current_frame_index();
            Ok(create_frame_response(&frame, index))
        }

        // === Context ===
        Command::Context { lines } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;

            // Get stack trace to find current position
            let frames = sess.stack_trace(1).await?;
            let frame = frames.first().ok_or_else(|| {
                Error::Internal("No stack frame available".to_string())
            })?;

            // Read source file
            let source_path = frame
                .source
                .as_ref()
                .and_then(|s| s.path.as_ref())
                .ok_or_else(|| Error::Internal("No source file available".to_string()))?;

            let source_lines = read_source_context(source_path, frame.line, lines)?;

            // Get locals
            let vars = sess.get_locals(Some(frame.id)).await.unwrap_or_default();
            let locals: Vec<VariableInfo> = vars
                .iter()
                .map(|v| VariableInfo {
                    name: v.name.clone(),
                    value: v.value.clone(),
                    type_name: v.type_name.clone(),
                    variables_reference: v.variables_reference,
                })
                .collect();

            let result = ContextResult {
                thread_id: sess.stopped_thread().unwrap_or(1),
                source: Some(source_path.clone()),
                line: frame.line,
                column: Some(frame.column),
                function: Some(frame.name.clone()),
                source_lines,
                locals,
            };

            Ok(serde_json::to_value(result)?)
        }

        // === Async ===
        Command::Await { timeout_secs } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;

            // First process any pending events
            sess.process_events().await?;

            // If already stopped, return immediately
            if sess.state() == SessionState::Stopped {
                return Ok(json!({
                    "reason": sess.stopped_reason().unwrap_or("unknown"),
                    "thread_id": sess.stopped_thread().unwrap_or(0),
                    "already_stopped": true
                }));
            }

            if sess.state() == SessionState::Exited {
                return Ok(json!({
                    "reason": "exited",
                    "exit_code": sess.exit_code().unwrap_or(0)
                }));
            }

            // Wait for stop event
            let event = sess.wait_stopped(timeout_secs).await?;

            match event {
                Event::Stopped(body) => {
                    let result = StopResult {
                        reason: body.reason,
                        description: body.description,
                        thread_id: body.thread_id.unwrap_or(0),
                        all_threads_stopped: body.all_threads_stopped,
                        hit_breakpoint_ids: body.hit_breakpoint_ids,
                        source: None, // Would need stack trace to get this
                        line: None,
                        column: None,
                    };
                    Ok(serde_json::to_value(result)?)
                }
                Event::Exited(body) => Ok(json!({
                    "reason": "exited",
                    "exit_code": body.exit_code
                })),
                Event::Terminated(_) => Ok(json!({
                    "reason": "terminated"
                })),
                _ => Ok(json!({
                    "reason": "unknown"
                })),
            }
        }

        // === Output ===
        Command::GetOutput { tail, clear } => {
            let sess = session.as_mut().ok_or(Error::SessionNotActive)?;
            let events = sess.get_output(tail, clear);

            let output: String = events.iter().map(|e| e.output.as_str()).collect();

            Ok(json!({
                "output": output,
                "count": events.len()
            }))
        }

        Command::SubscribeOutput => {
            // TODO: Implement output streaming
            Err(Error::Internal("Output streaming not yet implemented".to_string()))
        }

        // === Shutdown ===
        Command::Shutdown => {
            // Signal daemon to exit
            Ok(json!({ "shutdown": true }))
        }
    }
}

/// Create a JSON response for frame navigation commands
fn create_frame_response(frame: &crate::dap::StackFrame, index: usize) -> serde_json::Value {
    let frame_info = StackFrameInfo {
        id: frame.id,
        name: frame.name.clone(),
        source: frame.source.as_ref().and_then(|s| s.path.clone()),
        line: Some(frame.line),
        column: Some(frame.column),
    };

    json!({
        "selected": index,
        "frame": frame_info
    })
}

/// Read source file and return lines around the current position
fn read_source_context(path: &str, current_line: u32, context: usize) -> Result<Vec<SourceLine>> {
    let content = std::fs::read_to_string(path).map_err(|e| Error::FileRead {
        path: path.to_string(),
        error: e.to_string(),
    })?;

    let lines: Vec<&str> = content.lines().collect();
    let current_idx = (current_line as usize).saturating_sub(1);

    let start = current_idx.saturating_sub(context);
    let end = (current_idx + context + 1).min(lines.len());

    let mut result = Vec::new();
    for (idx, line) in lines[start..end].iter().enumerate() {
        let line_num = (start + idx + 1) as u32;
        result.push(SourceLine {
            number: line_num,
            content: (*line).to_string(),
            is_current: line_num == current_line,
        });
    }

    Ok(result)
}
