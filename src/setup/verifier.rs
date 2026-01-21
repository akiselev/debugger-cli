//! Installation verification
//!
//! Verifies that installed debuggers work correctly by sending DAP messages.

use crate::common::{Error, Result};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{timeout, Duration};

/// Result of verifying a debugger installation
#[derive(Debug, Clone)]
pub struct VerifyResult {
    /// Whether verification succeeded
    pub success: bool,
    /// Debugger capabilities if successful
    pub capabilities: Option<DapCapabilities>,
    /// Error message if verification failed
    pub error: Option<String>,
}

/// DAP capabilities (subset)
#[derive(Debug, Clone, Default)]
pub struct DapCapabilities {
    pub supports_configuration_done_request: bool,
    pub supports_function_breakpoints: bool,
    pub supports_conditional_breakpoints: bool,
    pub supports_evaluate_for_hovers: bool,
}

/// Verify a DAP adapter by sending initialize request
pub async fn verify_dap_adapter(
    path: &Path,
    args: &[String],
) -> Result<VerifyResult> {
    // Spawn the adapter
    let mut child = spawn_adapter(path, args).await?;

    // Send initialize request
    let init_result = timeout(Duration::from_secs(5), send_initialize(&mut child)).await;

    // Cleanup
    let _ = child.kill().await;

    match init_result {
        Ok(Ok(caps)) => Ok(VerifyResult {
            success: true,
            capabilities: Some(caps),
            error: None,
        }),
        Ok(Err(e)) => Ok(VerifyResult {
            success: false,
            capabilities: None,
            error: Some(e.to_string()),
        }),
        Err(_) => Ok(VerifyResult {
            success: false,
            capabilities: None,
            error: Some("Timeout waiting for adapter response".to_string()),
        }),
    }
}

/// Verify a TCP-based DAP adapter (like Delve) by spawning it with --listen
/// and connecting via TCP to send the initialize request
pub async fn verify_dap_adapter_tcp(
    path: &Path,
    args: &[String],
) -> Result<VerifyResult> {
    // Spawn the adapter with --listen=127.0.0.1:0
    let mut cmd = Command::new(path);
    cmd.args(args)
        .arg("--listen=127.0.0.1:0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        Error::Internal(format!("Failed to spawn adapter: {}", e))
    })?;

    // Read stdout to find the listening address
    let stdout = child.stdout.take().ok_or_else(|| {
        Error::Internal("Failed to get adapter stdout".to_string())
    })?;

    let mut stdout_reader = BufReader::new(stdout);
    let mut line = String::new();

    // Wait for the "listening at" message with timeout
    let listen_result = timeout(Duration::from_secs(10), async {
        loop {
            line.clear();
            let bytes_read = stdout_reader.read_line(&mut line).await.map_err(|e| {
                Error::Internal(format!("Failed to read adapter output: {}", e))
            })?;

            if bytes_read == 0 {
                return Err(Error::Internal(
                    "Adapter exited before outputting listen address".to_string(),
                ));
            }

            // Look for the listening address in the output
            if let Some(addr_start) = line.find("listening at:") {
                let addr_part = &line[addr_start + "listening at:".len()..];
                let addr = addr_part.trim().to_string();
                // Handle IPv6 format [::]:PORT
                let addr = if addr.starts_with("[::]:") {
                    addr.replace("[::]:", "127.0.0.1:")
                } else {
                    addr
                };
                return Ok(addr);
            }
        }
    })
    .await;

    let addr = match listen_result {
        Ok(Ok(addr)) => addr,
        Ok(Err(e)) => {
            let _ = child.kill().await;
            return Ok(VerifyResult {
                success: false,
                capabilities: None,
                error: Some(e.to_string()),
            });
        }
        Err(_) => {
            let _ = child.kill().await;
            return Ok(VerifyResult {
                success: false,
                capabilities: None,
                error: Some("Timeout waiting for adapter to start listening".to_string()),
            });
        }
    };

    // Connect to the TCP port
    let stream = match TcpStream::connect(&addr).await {
        Ok(s) => s,
        Err(e) => {
            let _ = child.kill().await;
            return Ok(VerifyResult {
                success: false,
                capabilities: None,
                error: Some(format!("Failed to connect to {}: {}", addr, e)),
            });
        }
    };

    // Send initialize request and wait for response
    let init_result = timeout(Duration::from_secs(5), send_initialize_tcp(stream)).await;

    // Cleanup
    let _ = child.kill().await;

    match init_result {
        Ok(Ok(caps)) => Ok(VerifyResult {
            success: true,
            capabilities: Some(caps),
            error: None,
        }),
        Ok(Err(e)) => Ok(VerifyResult {
            success: false,
            capabilities: None,
            error: Some(e.to_string()),
        }),
        Err(_) => Ok(VerifyResult {
            success: false,
            capabilities: None,
            error: Some("Timeout waiting for adapter response".to_string()),
        }),
    }
}

/// Spawn the adapter process
async fn spawn_adapter(path: &Path, args: &[String]) -> Result<Child> {
    let child = Command::new(path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped()) // Capture stderr for better error messages
        .spawn()
        .map_err(|e| Error::Internal(format!("Failed to spawn adapter: {}", e)))?;

    Ok(child)
}

/// Send DAP initialize request and parse response
async fn send_initialize(child: &mut Child) -> Result<DapCapabilities> {
    let stdin = child.stdin.as_mut().ok_or_else(|| {
        Error::Internal("Failed to get stdin".to_string())
    })?;
    let stdout = child.stdout.as_mut().ok_or_else(|| {
        Error::Internal("Failed to get stdout".to_string())
    })?;

    // Create initialize request
    let request = serde_json::json!({
        "seq": 1,
        "type": "request",
        "command": "initialize",
        "arguments": {
            "clientID": "debugger-cli",
            "clientName": "debugger-cli",
            "adapterID": "test",
            "pathFormat": "path",
            "linesStartAt1": true,
            "columnsStartAt1": true,
            "supportsRunInTerminalRequest": false
        }
    });

    // Send with DAP header
    let body = serde_json::to_string(&request)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    stdin.write_all(header.as_bytes()).await?;
    stdin.write_all(body.as_bytes()).await?;
    stdin.flush().await?;

    // Read response
    let mut reader = BufReader::new(stdout);

    // Parse DAP headers - some adapters emit multiple headers (Content-Length, Content-Type)
    let mut content_length: Option<usize> = None;
    loop {
        let mut header_line = String::new();
        reader.read_line(&mut header_line).await?;
        let trimmed = header_line.trim();

        // Empty line marks end of headers
        if trimmed.is_empty() {
            break;
        }

        // Parse Content-Length header
        if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
            content_length = len_str.trim().parse().ok();
        }
        // Ignore other headers (e.g., Content-Type)
    }

    let content_length = content_length
        .ok_or_else(|| Error::Internal("Missing Content-Length in DAP response".to_string()))?;

    // Read body
    let mut body = vec![0u8; content_length];
    tokio::io::AsyncReadExt::read_exact(&mut reader, &mut body).await?;

    // Parse response
    let response: serde_json::Value = serde_json::from_slice(&body)?;

    // Check for success
    if response.get("success").and_then(|v| v.as_bool()) != Some(true) {
        let message = response
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error");
        return Err(Error::Internal(format!("Initialize failed: {}", message)));
    }

    // Extract capabilities
    let body = response.get("body").cloned().unwrap_or_default();
    let caps = DapCapabilities {
        supports_configuration_done_request: body
            .get("supportsConfigurationDoneRequest")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        supports_function_breakpoints: body
            .get("supportsFunctionBreakpoints")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        supports_conditional_breakpoints: body
            .get("supportsConditionalBreakpoints")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        supports_evaluate_for_hovers: body
            .get("supportsEvaluateForHovers")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    };

    Ok(caps)
}

/// Send DAP initialize request via TCP and parse response
async fn send_initialize_tcp(stream: TcpStream) -> Result<DapCapabilities> {
    use tokio::io::AsyncReadExt;

    let (read_half, mut write_half) = tokio::io::split(stream);

    // Create initialize request
    let request = serde_json::json!({
        "seq": 1,
        "type": "request",
        "command": "initialize",
        "arguments": {
            "clientID": "debugger-cli",
            "clientName": "debugger-cli",
            "adapterID": "test",
            "pathFormat": "path",
            "linesStartAt1": true,
            "columnsStartAt1": true,
            "supportsRunInTerminalRequest": false
        }
    });

    // Send with DAP header
    let body = serde_json::to_string(&request)?;
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    write_half.write_all(header.as_bytes()).await?;
    write_half.write_all(body.as_bytes()).await?;
    write_half.flush().await?;

    // Read response
    let mut reader = BufReader::new(read_half);

    // Parse DAP headers
    let mut content_length: Option<usize> = None;
    loop {
        let mut header_line = String::new();
        reader.read_line(&mut header_line).await?;
        let trimmed = header_line.trim();

        // Empty line marks end of headers
        if trimmed.is_empty() {
            break;
        }

        // Parse Content-Length header
        if let Some(len_str) = trimmed.strip_prefix("Content-Length:") {
            content_length = len_str.trim().parse().ok();
        }
    }

    let content_length = content_length
        .ok_or_else(|| Error::Internal("Missing Content-Length in DAP response".to_string()))?;

    // Read body
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).await?;

    // Parse response
    let response: serde_json::Value = serde_json::from_slice(&body)?;

    // Check for success
    if response.get("success").and_then(|v| v.as_bool()) != Some(true) {
        let message = response
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown error");
        return Err(Error::Internal(format!("Initialize failed: {}", message)));
    }

    // Extract capabilities
    let body = response.get("body").cloned().unwrap_or_default();
    let caps = DapCapabilities {
        supports_configuration_done_request: body
            .get("supportsConfigurationDoneRequest")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        supports_function_breakpoints: body
            .get("supportsFunctionBreakpoints")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        supports_conditional_breakpoints: body
            .get("supportsConditionalBreakpoints")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        supports_evaluate_for_hovers: body
            .get("supportsEvaluateForHovers")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    };

    Ok(caps)
}

/// Simple executable check (just verifies the binary runs)
pub async fn verify_executable(path: &Path, version_arg: Option<&str>) -> Result<VerifyResult> {
    let arg = version_arg.unwrap_or("--version");

    let output = tokio::process::Command::new(path)
        .arg(arg)
        .output()
        .await
        .map_err(|e| Error::Internal(format!("Failed to run {}: {}", path.display(), e)))?;

    if output.status.success() {
        Ok(VerifyResult {
            success: true,
            capabilities: None,
            error: None,
        })
    } else {
        Ok(VerifyResult {
            success: false,
            capabilities: None,
            error: Some(format!(
                "Exit code: {}",
                output.status.code().unwrap_or(-1)
            )),
        })
    }
}
