//! Mock DAP adapter binary for integration testing
//!
//! This binary implements a minimal Debug Adapter Protocol server
//! that can be used for testing without requiring a real debugger.

use base64::Engine;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();

    let mut state = MockState::default();

    loop {
        // Read Content-Length header
        let mut header_line = String::new();
        if reader.read_line(&mut header_line).unwrap_or(0) == 0 {
            break; // EOF
        }

        if !header_line.starts_with("Content-Length:") {
            continue;
        }

        let content_length: usize = header_line
            .trim_start_matches("Content-Length:")
            .trim()
            .parse()
            .unwrap_or(0);

        // Read empty line
        let mut empty_line = String::new();
        reader.read_line(&mut empty_line).ok();

        // Read JSON body
        let mut body = vec![0u8; content_length];
        if reader.read_exact(&mut body).is_err() {
            break;
        }

        let message: Value = match serde_json::from_slice(&body) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Process message
        if let Some(responses) = state.process_message(&message) {
            for response in responses {
                send_message(&mut writer, &response);
            }
        }
    }
}

fn send_message<W: Write>(writer: &mut W, message: &Value) {
    let body = serde_json::to_string(message).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    writer.write_all(header.as_bytes()).ok();
    writer.write_all(body.as_bytes()).ok();
    writer.flush().ok();
}

struct MockState {
    seq: i64,
    initialized: bool,
    launched: bool,
    program: Option<String>,
    current_line: u32,
    current_file: String,
    stopped: bool,
    breakpoints: HashMap<String, Vec<u32>>,
    data_breakpoints: Vec<(String, String)>,
    variables: HashMap<String, (String, String)>, // name -> (value, type)
    stop_on_entry: bool,
}

impl Default for MockState {
    fn default() -> Self {
        let mut variables = HashMap::new();
        variables.insert("x".to_string(), ("42".to_string(), "int".to_string()));
        variables.insert("y".to_string(), ("3.14".to_string(), "double".to_string()));
        variables.insert(
            "name".to_string(),
            ("\"hello\"".to_string(), "const char*".to_string()),
        );

        Self {
            seq: 1,
            initialized: false,
            launched: false,
            program: None,
            current_line: 1,
            current_file: "main.c".to_string(),
            stopped: false,
            breakpoints: HashMap::new(),
            data_breakpoints: Vec::new(),
            variables,
            stop_on_entry: false,
        }
    }
}

impl MockState {
    fn next_seq(&mut self) -> i64 {
        let seq = self.seq;
        self.seq += 1;
        seq
    }

    fn process_message(&mut self, message: &Value) -> Option<Vec<Value>> {
        let msg_type = message.get("type")?.as_str()?;

        if msg_type != "request" {
            return None;
        }

        let command = message.get("command")?.as_str()?;
        let request_seq = message.get("seq")?.as_i64()?;
        let arguments = message.get("arguments").cloned().unwrap_or(json!({}));

        let mut responses = Vec::new();
        let seq = self.next_seq();

        let (success, body) = match command {
            "initialize" => {
                self.initialized = true;
                (
                    true,
                    json!({
                        "supportsConfigurationDoneRequest": true,
                        "supportsFunctionBreakpoints": true,
                        "supportsConditionalBreakpoints": true,
                        "supportsHitConditionalBreakpoints": true,
                        "supportsEvaluateForHovers": true,
                        "supportsSetVariable": true,
                        "supportsRestartRequest": true,
                        "supportsDataBreakpoints": true,
                        "supportsReadMemoryRequest": true,
                        "supportsDisassembleRequest": true,
                        "supportsTerminateRequest": true
                    }),
                )
            }
            "launch" => {
                self.launched = true;
                self.program = arguments
                    .get("program")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                self.stop_on_entry = arguments
                    .get("stopOnEntry")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if self.stop_on_entry {
                    self.stopped = true;
                }
                (true, json!(null))
            }
            "attach" => {
                self.launched = true;
                self.stopped = true;
                (true, json!(null))
            }
            "configurationDone" => {
                // Send initialized event
                let init_event_seq = self.next_seq();
                responses.push(json!({
                    "seq": init_event_seq,
                    "type": "event",
                    "event": "initialized"
                }));

                // If stop on entry, send stopped event
                if self.stop_on_entry {
                    let stopped_event_seq = self.next_seq();
                    responses.push(json!({
                        "seq": stopped_event_seq,
                        "type": "event",
                        "event": "stopped",
                        "body": {
                            "reason": "entry",
                            "threadId": 1,
                            "allThreadsStopped": true
                        }
                    }));
                }
                (true, json!(null))
            }
            "setBreakpoints" => {
                let source = arguments
                    .get("source")
                    .and_then(|s| s.get("path"))
                    .and_then(|p| p.as_str())
                    .unwrap_or("unknown");
                let bp_args = arguments
                    .get("breakpoints")
                    .and_then(|b| b.as_array())
                    .cloned()
                    .unwrap_or_default();

                let mut breakpoints = Vec::new();
                let mut lines = Vec::new();

                for (i, bp) in bp_args.iter().enumerate() {
                    let line = bp.get("line").and_then(|l| l.as_u64()).unwrap_or(1) as u32;
                    lines.push(line);
                    breakpoints.push(json!({
                        "id": i + 1,
                        "verified": true,
                        "line": line,
                        "source": {
                            "path": source
                        }
                    }));
                }

                self.breakpoints.insert(source.to_string(), lines);
                (true, json!({ "breakpoints": breakpoints }))
            }
            "setFunctionBreakpoints" => {
                let bp_args = arguments
                    .get("breakpoints")
                    .and_then(|b| b.as_array())
                    .cloned()
                    .unwrap_or_default();
                let breakpoints: Vec<Value> = bp_args
                    .iter()
                    .enumerate()
                    .map(|(i, bp)| {
                        let name = bp.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                        json!({
                            "id": 100 + i,
                            "verified": true,
                            "message": format!("Breakpoint at function {}", name)
                        })
                    })
                    .collect();
                (true, json!({ "breakpoints": breakpoints }))
            }
            "setDataBreakpoints" => {
                let bp_args = arguments
                    .get("breakpoints")
                    .and_then(|b| b.as_array())
                    .cloned()
                    .unwrap_or_default();
                self.data_breakpoints.clear();

                let breakpoints: Vec<Value> = bp_args
                    .iter()
                    .enumerate()
                    .map(|(i, bp)| {
                        let data_id = bp
                            .get("dataId")
                            .and_then(|d| d.as_str())
                            .unwrap_or("")
                            .to_string();
                        let access_type = bp
                            .get("accessType")
                            .and_then(|a| a.as_str())
                            .unwrap_or("write")
                            .to_string();
                        self.data_breakpoints
                            .push((data_id.clone(), access_type));
                        json!({
                            "id": 200 + i,
                            "verified": true,
                            "message": format!("Watchpoint on {}", data_id)
                        })
                    })
                    .collect();
                (true, json!({ "breakpoints": breakpoints }))
            }
            "dataBreakpointInfo" => {
                let name = arguments
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown");
                (
                    true,
                    json!({
                        "dataId": format!("&{}", name),
                        "description": format!("Address of variable '{}'", name),
                        "accessTypes": ["read", "write", "readWrite"],
                        "canPersist": true
                    }),
                )
            }
            "continue" => {
                self.stopped = false;
                self.current_line += 1;

                // Simulate hitting a breakpoint or finishing
                let stopped_event_seq = self.next_seq();
                self.stopped = true;
                responses.push(json!({
                    "seq": stopped_event_seq,
                    "type": "event",
                    "event": "stopped",
                    "body": {
                        "reason": "breakpoint",
                        "threadId": 1,
                        "allThreadsStopped": true,
                        "hitBreakpointIds": [1]
                    }
                }));

                (true, json!({ "allThreadsContinued": true }))
            }
            "next" | "stepIn" | "stepOut" => {
                self.stopped = false;
                self.current_line += 1;

                // Send stopped event
                let stopped_event_seq = self.next_seq();
                self.stopped = true;
                responses.push(json!({
                    "seq": stopped_event_seq,
                    "type": "event",
                    "event": "stopped",
                    "body": {
                        "reason": "step",
                        "threadId": 1,
                        "allThreadsStopped": true
                    }
                }));

                (true, json!(null))
            }
            "pause" => {
                self.stopped = true;
                (true, json!(null))
            }
            "threads" => (
                true,
                json!({
                    "threads": [
                        { "id": 1, "name": "main" }
                    ]
                }),
            ),
            "stackTrace" => (
                true,
                json!({
                    "stackFrames": [
                        {
                            "id": 1,
                            "name": "main",
                            "source": {
                                "name": &self.current_file,
                                "path": format!("/test/{}", &self.current_file)
                            },
                            "line": self.current_line,
                            "column": 1
                        },
                        {
                            "id": 2,
                            "name": "__libc_start_main",
                            "line": 0,
                            "column": 0
                        }
                    ],
                    "totalFrames": 2
                }),
            ),
            "scopes" => (
                true,
                json!({
                    "scopes": [
                        {
                            "name": "Locals",
                            "variablesReference": 1000,
                            "expensive": false
                        },
                        {
                            "name": "Globals",
                            "variablesReference": 2000,
                            "expensive": true
                        }
                    ]
                }),
            ),
            "variables" => {
                let var_ref = arguments
                    .get("variablesReference")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                let vars: Vec<Value> = if var_ref == 1000 {
                    self.variables
                        .iter()
                        .map(|(name, (value, type_name))| {
                            json!({
                                "name": name,
                                "value": value,
                                "type": type_name,
                                "variablesReference": 0
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                (true, json!({ "variables": vars }))
            }
            "evaluate" => {
                let expr = arguments
                    .get("expression")
                    .and_then(|e| e.as_str())
                    .unwrap_or("");
                let result = if let Some((value, type_name)) = self.variables.get(expr) {
                    json!({
                        "result": value,
                        "type": type_name,
                        "variablesReference": 0
                    })
                } else {
                    json!({
                        "result": format!("(eval result: {})", expr),
                        "type": "int",
                        "variablesReference": 0
                    })
                };
                (true, result)
            }
            "setVariable" => {
                let name = arguments
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("");
                let value = arguments
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if let Some((old_value, type_name)) = self.variables.get_mut(name) {
                    *old_value = value.to_string();
                    (
                        true,
                        json!({
                            "value": value,
                            "type": type_name,
                            "variablesReference": 0
                        }),
                    )
                } else {
                    (
                        false,
                        json!({ "message": format!("Variable '{}' not found", name) }),
                    )
                }
            }
            "readMemory" => {
                let count = arguments
                    .get("count")
                    .and_then(|c| c.as_u64())
                    .unwrap_or(64);
                let data = base64::engine::general_purpose::STANDARD.encode(vec![0u8; count as usize]);
                (
                    true,
                    json!({
                        "address": arguments.get("memoryReference").and_then(|m| m.as_str()).unwrap_or("0x0"),
                        "data": data,
                        "unreadableBytes": 0
                    }),
                )
            }
            "disassemble" => {
                let count = arguments
                    .get("instructionCount")
                    .and_then(|c| c.as_u64())
                    .unwrap_or(10);

                let instructions: Vec<Value> = (0..count)
                    .map(|i| {
                        let offset = i * 4;
                        json!({
                            "address": format!("0x{:x}", 0x1000 + offset),
                            "instruction": format!("mov r{}, #{}", i % 8, i),
                            "instructionBytes": "00 00 00 00"
                        })
                    })
                    .collect();

                (true, json!({ "instructions": instructions }))
            }
            "restart" => {
                self.current_line = 1;
                self.stopped = true;

                let stopped_event_seq = self.next_seq();
                responses.push(json!({
                    "seq": stopped_event_seq,
                    "type": "event",
                    "event": "stopped",
                    "body": {
                        "reason": "entry",
                        "threadId": 1,
                        "allThreadsStopped": true
                    }
                }));

                (true, json!(null))
            }
            "disconnect" => {
                self.launched = false;
                self.stopped = false;
                (true, json!(null))
            }
            "terminate" => (true, json!(null)),
            _ => (
                false,
                json!({ "message": format!("Unknown command: {}", command) }),
            ),
        };

        // Insert the main response at the beginning
        responses.insert(
            0,
            json!({
                "seq": seq,
                "type": "response",
                "request_seq": request_seq,
                "success": success,
                "command": command,
                "body": body
            }),
        );

        Some(responses)
    }
}
