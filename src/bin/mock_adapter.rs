//! Mock DAP adapter for integration testing
//!
//! A minimal DAP adapter that responds with predictable, scripted responses.
//! This allows testing the debugger CLI without requiring a real debug adapter.

use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};

fn main() {
    // Mock adapter runs as a subprocess, reading from stdin and writing to stdout
    let stdin = io::stdin();
    let stdout = io::stdout();

    let reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());

    let mut adapter = MockAdapter::new();

    // Process DAP messages
    for line_result in reader.lines() {
        if let Ok(line) = line_result {
            // DAP messages are preceded by "Content-Length: N\r\n\r\n"
            if line.starts_with("Content-Length:") {
                let len: usize = line.trim_start_matches("Content-Length:").trim().parse().unwrap_or(0);

                // Read empty line
                let mut buf = String::new();
                let stdin = io::stdin();
                let mut handle = stdin.lock();
                let _ = handle.read_line(&mut buf);

                // Read JSON content
                let mut json_buf = vec![0u8; len];
                let _ = handle.read_exact(&mut json_buf);

                if let Ok(json_str) = String::from_utf8(json_buf) {
                    if let Ok(msg) = serde_json::from_str::<Value>(&json_str) {
                        if let Some(responses) = adapter.handle_message(&msg) {
                            for response in responses {
                                write_message(&mut writer, &response).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
}

fn write_message<W: Write>(writer: &mut W, msg: &Value) -> io::Result<()> {
    let json = serde_json::to_string(msg)?;
    write!(writer, "Content-Length: {}\r\n\r\n{}", json.len(), json)?;
    writer.flush()
}

struct MockAdapter {
    seq: i64,
    initialized: bool,
    launched: bool,
    stopped: bool,
}

impl MockAdapter {
    fn new() -> Self {
        Self {
            seq: 1,
            initialized: false,
            launched: false,
            stopped: false,
        }
    }

    fn next_seq(&mut self) -> i64 {
        let s = self.seq;
        self.seq += 1;
        s
    }

    fn handle_message(&mut self, msg: &Value) -> Option<Vec<Value>> {
        let msg_type = msg.get("type")?.as_str()?;

        match msg_type {
            "request" => self.handle_request(msg),
            _ => None,
        }
    }

    fn handle_request(&mut self, msg: &Value) -> Option<Vec<Value>> {
        let command = msg.get("command")?.as_str()?;
        let request_seq = msg.get("seq")?.as_i64()?;

        match command {
            "initialize" => {
                let response = self.success_response(request_seq, command, json!({
                    "supportsConfigurationDoneRequest": true,
                    "supportsFunctionBreakpoints": true,
                    "supportsConditionalBreakpoints": true,
                    "supportsHitConditionalBreakpoints": true,
                    "supportsEvaluateForHovers": true,
                    "supportsDataBreakpoints": true,
                    "supportsReadMemoryRequest": true,
                }));
                Some(vec![response])
            }

            "launch" => {
                self.launched = true;
                let response = self.success_response(request_seq, command, json!({}));
                let initialized_event = self.event("initialized", json!({}));
                Some(vec![response, initialized_event])
            }

            "configurationDone" => {
                // Send a stopped event (entry point stop)
                let response = self.success_response(request_seq, command, json!({}));
                let stopped_event = self.event("stopped", json!({
                    "reason": "entry",
                    "threadId": 1,
                    "allThreadsStopped": true,
                }));
                self.stopped = true;
                Some(vec![response, stopped_event])
            }

            "setBreakpoints" => {
                let args = msg.get("arguments")?;
                let breakpoints = args.get("breakpoints")?.as_array()?;

                let response_bps: Vec<Value> = breakpoints.iter().enumerate().map(|(i, bp)| {
                    json!({
                        "id": i + 1,
                        "verified": true,
                        "line": bp.get("line").and_then(|v| v.as_u64()).unwrap_or(1),
                    })
                }).collect();

                Some(vec![self.success_response(request_seq, command, json!({
                    "breakpoints": response_bps
                }))])
            }

            "setFunctionBreakpoints" => {
                let args = msg.get("arguments")?;
                let breakpoints = args.get("breakpoints")?.as_array()?;

                let response_bps: Vec<Value> = breakpoints.iter().enumerate().map(|(i, _)| {
                    json!({
                        "id": i + 100,
                        "verified": true,
                    })
                }).collect();

                Some(vec![self.success_response(request_seq, command, json!({
                    "breakpoints": response_bps
                }))])
            }

            "threads" => {
                Some(vec![self.success_response(request_seq, command, json!({
                    "threads": [
                        { "id": 1, "name": "main" }
                    ]
                }))])
            }

            "stackTrace" => {
                Some(vec![self.success_response(request_seq, command, json!({
                    "stackFrames": [
                        {
                            "id": 1,
                            "name": "main",
                            "source": { "path": "/tmp/test.c" },
                            "line": 5,
                            "column": 1
                        }
                    ],
                    "totalFrames": 1
                }))])
            }

            "scopes" => {
                Some(vec![self.success_response(request_seq, command, json!({
                    "scopes": [
                        {
                            "name": "Locals",
                            "variablesReference": 1000,
                            "expensive": false
                        }
                    ]
                }))])
            }

            "variables" => {
                Some(vec![self.success_response(request_seq, command, json!({
                    "variables": [
                        {
                            "name": "x",
                            "value": "42",
                            "type": "int",
                            "variablesReference": 0
                        },
                        {
                            "name": "str",
                            "value": "\"hello\"",
                            "type": "char *",
                            "variablesReference": 0
                        }
                    ]
                }))])
            }

            "evaluate" => {
                let args = msg.get("arguments")?;
                let expression = args.get("expression")?.as_str()?;

                Some(vec![self.success_response(request_seq, command, json!({
                    "result": format!("(eval) {}", expression),
                    "variablesReference": 0
                }))])
            }

            "continue" => {
                self.stopped = false;
                // Simulate running then stopping at a breakpoint
                let response = self.success_response(request_seq, command, json!({
                    "allThreadsContinued": true
                }));
                let stopped_event = self.event("stopped", json!({
                    "reason": "breakpoint",
                    "threadId": 1,
                    "allThreadsStopped": true,
                    "hitBreakpointIds": [1]
                }));
                self.stopped = true;
                Some(vec![response, stopped_event])
            }

            "next" | "stepIn" | "stepOut" => {
                let response = self.success_response(request_seq, command, json!({}));
                let stopped_event = self.event("stopped", json!({
                    "reason": "step",
                    "threadId": 1,
                    "allThreadsStopped": true
                }));
                Some(vec![response, stopped_event])
            }

            "pause" => {
                self.stopped = true;
                let response = self.success_response(request_seq, command, json!({}));
                let stopped_event = self.event("stopped", json!({
                    "reason": "pause",
                    "threadId": 1,
                    "allThreadsStopped": true
                }));
                Some(vec![response, stopped_event])
            }

            "readMemory" => {
                // Return some mock memory data
                let args = msg.get("arguments")?;
                let count = args.get("count")?.as_u64().unwrap_or(16);

                // Generate mock data (0x00 to 0xff repeating)
                let data: Vec<u8> = (0..count).map(|i| (i % 256) as u8).collect();
                let b64_data = base64_encode(&data);

                Some(vec![self.success_response(request_seq, command, json!({
                    "address": args.get("memoryReference").unwrap_or(&json!("0x1000")),
                    "data": b64_data
                }))])
            }

            "dataBreakpointInfo" => {
                let args = msg.get("arguments")?;
                let name = args.get("name")?.as_str()?;

                Some(vec![self.success_response(request_seq, command, json!({
                    "dataId": format!("watch:{}", name),
                    "description": format!("Watch on variable '{}'", name),
                    "accessTypes": ["read", "write", "readWrite"]
                }))])
            }

            "setDataBreakpoints" => {
                let args = msg.get("arguments")?;
                let breakpoints = args.get("breakpoints")?.as_array()?;

                let response_bps: Vec<Value> = breakpoints.iter().enumerate().map(|(i, _)| {
                    json!({
                        "id": i + 200,
                        "verified": true,
                    })
                }).collect();

                Some(vec![self.success_response(request_seq, command, json!({
                    "breakpoints": response_bps
                }))])
            }

            "disconnect" => {
                let response = self.success_response(request_seq, command, json!({}));
                let terminated_event = self.event("terminated", json!({}));
                Some(vec![response, terminated_event])
            }

            _ => {
                // Unknown command - return error
                Some(vec![self.error_response(request_seq, command, &format!("Unknown command: {}", command))])
            }
        }
    }

    fn success_response(&mut self, request_seq: i64, command: &str, body: Value) -> Value {
        json!({
            "seq": self.next_seq(),
            "type": "response",
            "request_seq": request_seq,
            "success": true,
            "command": command,
            "body": body
        })
    }

    fn error_response(&mut self, request_seq: i64, command: &str, message: &str) -> Value {
        json!({
            "seq": self.next_seq(),
            "type": "response",
            "request_seq": request_seq,
            "success": false,
            "command": command,
            "message": message
        })
    }

    fn event(&mut self, event: &str, body: Value) -> Value {
        json!({
            "seq": self.next_seq(),
            "type": "event",
            "event": event,
            "body": body
        })
    }
}

// Simple base64 encoding without external dependency
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut result = String::new();
    let mut chunks = data.chunks_exact(3);

    for chunk in chunks.by_ref() {
        let n = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        result.push(ALPHABET[(n >> 18) as usize & 0x3F] as char);
        result.push(ALPHABET[(n >> 12) as usize & 0x3F] as char);
        result.push(ALPHABET[(n >> 6) as usize & 0x3F] as char);
        result.push(ALPHABET[n as usize & 0x3F] as char);
    }

    let remainder = chunks.remainder();
    if !remainder.is_empty() {
        let mut n = (remainder[0] as u32) << 16;
        if remainder.len() > 1 {
            n |= (remainder[1] as u32) << 8;
        }

        result.push(ALPHABET[(n >> 18) as usize & 0x3F] as char);
        result.push(ALPHABET[(n >> 12) as usize & 0x3F] as char);
        if remainder.len() > 1 {
            result.push(ALPHABET[(n >> 6) as usize & 0x3F] as char);
        } else {
            result.push('=');
        }
        result.push('=');
    }

    result
}
