# Developer Guide

Welcome to debugger-cli! This guide covers everything you need to contribute to the project.

> **Quick Links**: [README](../README.md) | [Changelog](../CHANGELOG.md) | [Architecture](plan/architecture.md)

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Project Structure](#project-structure)
3. [Command Flow](#command-flow)
4. [Adding New Commands](#adding-new-commands)
5. [Working with the DAP Client](#working-with-the-dap-client)
6. [IPC Protocol](#ipc-protocol)
7. [Error Handling](#error-handling)
8. [Configuration System](#configuration-system)
9. [Testing](#testing)
10. [Debugging Tips](#debugging-tips)

## Prerequisites

- Rust 1.70+ (`rustup update stable`)
- A debug adapter for testing (see [README](../README.md#prerequisites))
- Optional: `lldb-dap` or `codelldb` for end-to-end testing

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run -- status
```

---

## Architecture Overview

The debugger-cli implements an LLM-friendly debugger using the Debug Adapter Protocol (DAP). The key architectural insight is that **a single binary runs in two modes**:

```
┌─────────────────┐                    ┌─────────────────┐
│   CLI Mode      │  ───IPC Socket───▶ │  Daemon Mode    │
│  (thin client)  │                    │ (state manager) │
└─────────────────┘                    └────────┬────────┘
                                                │
                                                │ stdio
                                                ▼
                                       ┌─────────────────┐
                                       │  DAP Adapter    │
                                       │ (lldb-dap, etc) │
                                       └─────────────────┘
```

- **CLI Mode**: Parses user commands and forwards them to the daemon via IPC
- **Daemon Mode**: Long-running process that manages the debug session and communicates with the DAP adapter

This separation allows:
- Persistent debug sessions across multiple CLI invocations
- Non-blocking command execution
- Clean process lifecycle management

---

## Project Structure

```
src/
├── main.rs              # Entry point: dispatches CLI vs daemon mode
├── commands.rs          # Clap command definitions (CLI argument parsing)
├── lib.rs               # Library exports
│
├── cli/                 # CLI-side code (thin client)
│   ├── mod.rs           # dispatch() routes commands to handlers
│   └── spawn.rs         # Daemon spawning and management
│
├── daemon/              # Daemon-side code (state manager)
│   ├── mod.rs           # run() entry point for daemon mode
│   ├── server.rs        # IPC listener loop, accepts client connections
│   ├── handler.rs       # Command handler dispatcher
│   └── session.rs       # Debug session state machine & DAP orchestration
│
├── dap/                 # DAP client implementation
│   ├── client.rs        # DapClient: spawns adapter, sends requests
│   ├── codec.rs         # Wire protocol (Content-Length framing)
│   └── types.rs         # DAP message types (requests, responses, events)
│
├── ipc/                 # CLI ↔ Daemon communication
│   ├── protocol.rs      # Request/Response types, Command enum
│   ├── client.rs        # DaemonClient for CLI side
│   └── transport.rs     # Cross-platform socket/pipe implementation
│
└── common/              # Shared utilities
    ├── config.rs        # TOML config file loading
    ├── error.rs         # Error types (thiserror)
    └── paths.rs         # Platform-specific paths (socket, config)

tests/
├── integration.rs       # End-to-end tests
└── fixtures/            # Test programs (C, Rust)
    └── simple.c
```

---

## Command Flow

Understanding how a command flows through the system is essential. Here's what happens when you run `debugger break src/main.rs:42`:

### 1. CLI Parsing (`src/main.rs`)

```rust
// main.rs:18-40
let cli = Cli::parse();  // clap parses args
match cli.command {
    Commands::Daemon => daemon::run().await,  // Start as daemon
    command => cli::dispatch(command).await,  // Handle as CLI command
}
```

### 2. CLI Dispatch (`src/cli/mod.rs`)

```rust
// cli/mod.rs - dispatch() function
Commands::Break { location, condition } => {
    ensure_daemon_running().await?;  // Spawn daemon if needed
    let mut client = DaemonClient::connect().await?;

    let loc = BreakpointLocation::parse(&location)?;
    let result = client.send_command(Command::BreakpointAdd {
        location: loc,
        condition,
        hit_count: None,
    }).await?;

    // Format and print result
}
```

### 3. IPC Transport (`src/ipc/`)

The command is serialized as JSON and sent over a Unix socket (or named pipe on Windows):

```json
{
  "id": 1,
  "command": {
    "type": "breakpoint_add",
    "location": { "type": "line", "file": "src/main.rs", "line": 42 }
  }
}
```

### 4. Daemon Handler (`src/daemon/handler.rs`)

```rust
// handler.rs - handle_command_inner()
Command::BreakpointAdd { location, condition, hit_count } => {
    let session = require_session(session)?;
    let bp = session.add_breakpoint(location, condition, hit_count).await?;
    Ok(json!(BreakpointInfo::from(bp)))
}
```

### 5. Session → DAP Client (`src/daemon/session.rs`, `src/dap/client.rs`)

```rust
// session.rs - add_breakpoint()
pub async fn add_breakpoint(&mut self, location: BreakpointLocation, ...) -> Result<...> {
    // Store breakpoint locally
    self.source_breakpoints.entry(file.clone()).or_default().push(stored);

    // Send to DAP adapter
    let response = self.client.set_breakpoints(&file, &breakpoints).await?;

    // Update with adapter's response (verified status, actual line)
}

// client.rs - set_breakpoints()
pub async fn set_breakpoints(&mut self, source: &Path, breakpoints: &[SourceBreakpoint]) -> Result<...> {
    self.request_with_timeout("setBreakpoints", Some(json!({
        "source": { "path": source },
        "breakpoints": breakpoints
    })), self.request_timeout).await
}
```

### 6. Response Flows Back

The response travels back through the same path: DAP → Session → Handler → IPC → CLI → User output.

---

## Adding New Commands

Adding a new command involves changes to 4-5 files. Here's a step-by-step guide:

### Step 1: Define CLI Arguments (`src/commands.rs`)

Add your command to the `Commands` enum:

```rust
#[derive(Subcommand)]
pub enum Commands {
    // ... existing commands ...

    /// Your new command description (shown in --help)
    #[command(name = "mycommand")]
    MyCommand {
        /// Argument description
        #[arg(long, short)]
        some_arg: String,

        /// Optional argument with default
        #[arg(long, default_value = "10")]
        limit: u32,
    },
}
```

### Step 2: Define IPC Protocol (`src/ipc/protocol.rs`)

Add the command variant to the `Command` enum:

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    // ... existing commands ...

    MyCommand {
        some_arg: String,
        limit: u32,
    },
}
```

If your command returns structured data, add a result type:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MyCommandResult {
    pub field1: String,
    pub field2: Vec<i32>,
}
```

### Step 3: Implement CLI Handler (`src/cli/mod.rs`)

Add your command to the `dispatch()` function:

```rust
pub async fn dispatch(command: Commands) -> Result<()> {
    match command {
        // ... existing handlers ...

        Commands::MyCommand { some_arg, limit } => {
            ensure_daemon_running().await?;
            let mut client = DaemonClient::connect().await?;

            let result = client.send_command(Command::MyCommand {
                some_arg,
                limit,
            }).await?;

            // Parse and display result
            let data: MyCommandResult = serde_json::from_value(result)?;
            println!("Result: {}", data.field1);
            for item in data.field2 {
                println!("  - {}", item);
            }
        }
    }
    Ok(())
}
```

### Step 4: Implement Daemon Handler (`src/daemon/handler.rs`)

Add your command to `handle_command_inner()`:

```rust
async fn handle_command_inner(
    session: &mut Option<DebugSession>,
    config: &Config,
    command: Command,
) -> Result<serde_json::Value> {
    match command {
        // ... existing handlers ...

        Command::MyCommand { some_arg, limit } => {
            let session = require_session(session)?;
            let result = session.my_command(&some_arg, limit).await?;
            Ok(serde_json::to_value(result)?)
        }
    }
}
```

### Step 5: Implement Session Logic (`src/daemon/session.rs`)

Add the method to `DebugSession`:

```rust
impl DebugSession {
    pub async fn my_command(&mut self, some_arg: &str, limit: u32) -> Result<MyCommandResult> {
        // Validate state if needed
        self.require_stopped()?;

        // Make DAP requests if needed
        let response = self.client.some_dap_request(...).await?;

        // Process and return result
        Ok(MyCommandResult {
            field1: some_arg.to_string(),
            field2: vec![1, 2, 3],
        })
    }
}
```

### Complete Example: Adding a "memory read" Command

```rust
// commands.rs
Commands::Memory {
    #[arg(help = "Address to read (hex)")]
    address: String,
    #[arg(long, default_value = "64")]
    count: u32,
},

// ipc/protocol.rs
Command::ReadMemory { address: String, count: u32 },

#[derive(Debug, Serialize, Deserialize)]
pub struct MemoryResult {
    pub address: String,
    pub data: Vec<u8>,
}

// cli/mod.rs
Commands::Memory { address, count } => {
    ensure_daemon_running().await?;
    let mut client = DaemonClient::connect().await?;
    let result = client.send_command(Command::ReadMemory { address, count }).await?;
    let mem: MemoryResult = serde_json::from_value(result)?;
    println!("{}: {:02x?}", mem.address, mem.data);
}

// daemon/handler.rs
Command::ReadMemory { address, count } => {
    let session = require_session(session)?;
    let result = session.read_memory(&address, count).await?;
    Ok(serde_json::to_value(result)?)
}

// daemon/session.rs
pub async fn read_memory(&mut self, address: &str, count: u32) -> Result<MemoryResult> {
    let addr = u64::from_str_radix(address.trim_start_matches("0x"), 16)
        .map_err(|_| Error::InvalidLocation(address.to_string()))?;

    let response: ReadMemoryResponse = self.client.request_with_timeout(
        "readMemory",
        Some(json!({
            "memoryReference": format!("0x{:x}", addr),
            "count": count
        })),
        self.request_timeout(),
    ).await?;

    Ok(MemoryResult {
        address: address.to_string(),
        data: base64::decode(&response.data)?,
    })
}
```

---

## Working with the DAP Client

The `DapClient` (`src/dap/client.rs`) handles all communication with debug adapters.

### Spawning the Adapter

```rust
let client = DapClient::spawn(&adapter_path, &adapter_args).await?;
```

This:
1. Spawns the adapter as a subprocess with stdin/stdout pipes
2. Starts a background reader task for async event handling
3. Returns a client ready for the initialize handshake

### Sending Requests

Use `request_with_timeout<T>()` for type-safe requests:

```rust
// Generic request with typed response
let response: StackTraceResponseBody = self.client.request_with_timeout(
    "stackTrace",
    Some(json!({
        "threadId": thread_id,
        "startFrame": 0,
        "levels": 20
    })),
    Duration::from_secs(30),
).await?;

// Access typed fields
for frame in response.stack_frames {
    println!("{}: {} at {}:{}", frame.id, frame.name,
             frame.source.map(|s| s.path).flatten().unwrap_or_default(),
             frame.line);
}
```

### Common DAP Requests

| Request | Purpose | Key Arguments |
|---------|---------|---------------|
| `initialize` | Handshake, exchange capabilities | `clientID`, `adapterID`, `supportsXxx` |
| `launch` | Start debugging a program | `program`, `args`, `stopOnEntry` |
| `attach` | Attach to running process | `pid` |
| `setBreakpoints` | Set breakpoints in a file | `source`, `breakpoints[]` |
| `setFunctionBreakpoints` | Set function breakpoints | `breakpoints[]` |
| `configurationDone` | Signal ready to run | (none) |
| `continue` | Resume execution | `threadId` |
| `next` | Step over | `threadId` |
| `stepIn` | Step into | `threadId` |
| `stepOut` | Step out | `threadId` |
| `pause` | Pause execution | `threadId` |
| `stackTrace` | Get call stack | `threadId`, `levels` |
| `scopes` | Get variable scopes | `frameId` |
| `variables` | Get variables | `variablesReference` |
| `evaluate` | Evaluate expression | `expression`, `frameId`, `context` |
| `threads` | List threads | (none) |
| `disconnect` | End session | `terminateDebuggee` |

### Handling Events

Events are received asynchronously by a background task and queued in a channel:

```rust
// In session.rs - take the event receiver
let events_rx = client.take_event_receiver()?;

// Process events
while let Ok(event) = events_rx.try_recv() {
    match event {
        Event::Stopped(body) => {
            self.state = SessionState::Stopped;
            self.stopped_thread = body.thread_id;
            self.stopped_reason = Some(body.reason);
        }
        Event::Output(body) => {
            self.buffer_output(&body.category.unwrap_or_default(), &body.output);
        }
        Event::Exited(body) => {
            self.state = SessionState::Exited;
            self.exit_code = Some(body.exit_code);
        }
        Event::Terminated(_) => {
            self.state = SessionState::Exited;
        }
        _ => {}
    }
}
```

### Key Event Types

| Event | When | Key Fields |
|-------|------|------------|
| `initialized` | Adapter ready for configuration | (none) |
| `stopped` | Execution stopped | `reason`, `threadId`, `hitBreakpointIds` |
| `continued` | Execution resumed | `threadId` |
| `output` | Program output | `category` (stdout/stderr), `output` |
| `thread` | Thread created/exited | `reason`, `threadId` |
| `exited` | Program exited | `exitCode` |
| `terminated` | Debug session ended | (none) |
| `breakpoint` | Breakpoint changed | `reason`, `breakpoint` |

### Race Condition Prevention

When sending requests, always register the response handler **before** sending:

```rust
// CORRECT: Register handler first
let (tx, rx) = oneshot::channel();
pending_guard.insert(seq, tx);  // Register BEFORE send

codec::write_message(&mut self.writer, &json).await?;  // Then send

let response = rx.await?;  // Wait for response

// WRONG: Send first, then register (race condition!)
// codec::write_message(&mut self.writer, &json).await?;
// pending_guard.insert(seq, tx);  // Too late! Response may have arrived
```

---

## IPC Protocol

Communication between CLI and daemon uses a simple length-prefixed JSON protocol.

### Message Format

```
┌─────────────────┬─────────────────────────────────┐
│ Length (4 bytes)│ JSON Payload (variable)         │
│ Little-endian   │                                 │
└─────────────────┴─────────────────────────────────┘
```

### Request Structure

```rust
pub struct Request {
    pub id: u64,           // For request-response correlation
    pub command: Command,  // The command enum
}
```

### Response Structure

```rust
pub struct Response {
    pub id: u64,                          // Matches request ID
    pub success: bool,
    pub result: Option<serde_json::Value>, // On success
    pub error: Option<IpcError>,           // On failure
}
```

### Adding New Protocol Types

When adding new commands that need structured results:

```rust
// 1. Define the result type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewResult {
    pub field: String,
}

// 2. Serialize in handler
Ok(serde_json::to_value(NewResult { field: "value".into() })?)

// 3. Deserialize in CLI
let result: NewResult = serde_json::from_value(response)?;
```

---

## Error Handling

### Error Types (`src/common/error.rs`)

The project uses `thiserror` for ergonomic error definitions:

```rust
#[derive(Error, Debug)]
pub enum Error {
    // Session errors
    #[error("No debug session active. Start one with 'debugger start <program>'")]
    SessionNotActive,

    // DAP errors
    #[error("DAP request '{command}' failed: {message}")]
    DapRequestFailed { command: String, message: String },

    // State errors
    #[error("Cannot {action} while program is {state}")]
    InvalidState { action: String, state: String },

    // ... many more
}
```

### Creating New Error Variants

```rust
// Add to Error enum
#[error("My new error: {0}")]
MyNewError(String),

// Add helper method
impl Error {
    pub fn my_new_error(detail: &str) -> Self {
        Self::MyNewError(detail.to_string())
    }
}

// Usage
return Err(Error::my_new_error("something went wrong"));
```

### IPC Error Conversion

Errors are converted to `IpcError` for transmission:

```rust
// In error.rs
impl From<&Error> for IpcError {
    fn from(e: &Error) -> Self {
        let code = match e {
            Error::SessionNotActive => "SESSION_NOT_ACTIVE",
            Error::MyNewError(_) => "MY_NEW_ERROR",  // Add your error code
            _ => "INTERNAL_ERROR",
        };
        Self { code: code.to_string(), message: e.to_string() }
    }
}
```

### Error Handling Patterns

```rust
// In handlers - use ? operator, errors become IPC responses
pub async fn my_handler(session: &mut Option<DebugSession>) -> Result<Value> {
    let session = require_session(session)?;  // Returns error if no session
    let result = session.do_thing().await?;   // Propagates DAP errors
    Ok(json!(result))
}

// In CLI - display errors nicely
if let Err(e) = result {
    eprintln!("Error: {e}");
    std::process::exit(1);
}
```

---

## Configuration System

### Config File Location

| Platform | Path |
|----------|------|
| Linux | `~/.config/debugger-cli/config.toml` |
| macOS | `~/Library/Application Support/debugger-cli/config.toml` |
| Windows | `%APPDATA%\debugger-cli\config.toml` |

### Config Structure

```toml
# Adapter configurations
[adapters.lldb-dap]
path = "lldb-dap"
args = []

[adapters.codelldb]
path = "/path/to/codelldb"
args = ["--port", "13000"]

# Default settings
[defaults]
adapter = "lldb-dap"

# Timeout settings (seconds)
[timeouts]
dap_initialize_secs = 10
dap_request_secs = 30
await_default_secs = 300

# Daemon settings
[daemon]
idle_timeout_minutes = 30

# Output buffer limits
[output]
max_events = 10000
max_bytes_mb = 10
```

### Accessing Config

```rust
// Load config (returns defaults if file missing)
let config = Config::load()?;

// Access adapter config
if let Some(adapter) = config.get_adapter("lldb-dap") {
    let path = &adapter.path;
    let args = &adapter.args;
}

// Access timeouts
let timeout = Duration::from_secs(config.timeouts.dap_request_secs);

// Access output limits
let max_bytes = config.output.max_bytes_mb * 1024 * 1024;
```

### Adding New Config Options

```rust
// In config.rs

// 1. Add field to appropriate struct
#[derive(Debug, Deserialize)]
pub struct MySection {
    #[serde(default = "default_my_option")]
    pub my_option: u32,
}

// 2. Add default function
fn default_my_option() -> u32 { 42 }

// 3. Add section to Config
pub struct Config {
    #[serde(default)]
    pub my_section: MySection,
}
```

---

## Testing

### Unit Tests

Run unit tests:

```bash
cargo test
```

Unit tests are in the same files as the code they test:

```rust
// src/ipc/protocol.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_file_line() {
        let loc = BreakpointLocation::parse("src/main.rs:42").unwrap();
        match loc {
            BreakpointLocation::Line { file, line } => {
                assert_eq!(file.to_string_lossy(), "src/main.rs");
                assert_eq!(line, 42);
            }
            _ => panic!("Expected Line variant"),
        }
    }
}
```

### Integration Tests

Integration tests are in `tests/integration.rs`. They require a debug adapter:

```bash
# Run all tests (integration tests are ignored by default)
cargo test

# Run integration tests (requires lldb-dap)
cargo test -- --ignored
```

### Test Fixtures

Test fixtures are C/Rust programs in `tests/fixtures/`:

```c
// tests/fixtures/simple.c
int add(int a, int b) {
    // BREAKPOINT_MARKER: add_body
    return a + b;
}

int main() {
    // BREAKPOINT_MARKER: main_start
    int x = 10;
    int y = 20;
    // BREAKPOINT_MARKER: before_add
    int sum = add(x, y);
    return 0;
}
```

Breakpoint markers allow tests to find specific lines:

```rust
let markers = ctx.find_breakpoint_markers(&ctx.fixtures_dir.join("simple.c"));
let line = markers.get("main_start").unwrap();
ctx.run_debugger_ok(&["break", &format!("simple.c:{}", line)]);
```

### Test Artifact Cleanup

By default, test artifacts are preserved for debugging. To clean up:

```bash
PRESERVE_DEBUGGER_TEST_ARTIFACTS=0 cargo test
```

---

## Debugging Tips

### Enable Tracing

The project uses the `tracing` crate for logging:

```bash
# Show all logs
RUST_LOG=debug cargo run -- start ./myprogram

# Show only DAP messages
RUST_LOG=debugger::dap=trace cargo run -- start ./myprogram

# Show daemon logs
RUST_LOG=debugger::daemon=debug cargo run daemon
```

### Inspect DAP Messages

DAP messages are logged at trace level:

```bash
RUST_LOG=trace cargo run -- start ./myprogram 2>&1 | grep "DAP"
```

Output:
```
DAP >>> {"seq":1,"type":"request","command":"initialize",...}
DAP <<< {"seq":1,"type":"response","request_seq":1,"success":true,...}
```

### Debug the Daemon

Run the daemon in foreground:

```bash
# Terminal 1: Start daemon manually
RUST_LOG=debug cargo run -- daemon

# Terminal 2: Run CLI commands
cargo run -- start ./myprogram
cargo run -- break main
cargo run -- continue
```

### Common Issues

**"Daemon not running"**
- The socket file may be stale. Delete it: `rm /tmp/debugger-cli-*/daemon.sock`
- Or use XDG runtime dir: `rm $XDG_RUNTIME_DIR/debugger-cli/daemon.sock`

**"Adapter not found"**
- Check adapter is in PATH: `which lldb-dap`
- Or specify full path in config.toml

**"DAP request timeout"**
- Adapter may have crashed. Check stderr output
- Increase timeout in config.toml

**"Session not active"**
- Start a session first: `debugger start ./program`
- Check if daemon is running: `debugger status`

### Useful Commands During Development

```bash
# Check daemon status
cargo run -- status

# Stop daemon (and debug session)
cargo run -- stop

# Force kill daemon
pkill -f "debugger daemon"

# Watch daemon socket
ls -la /tmp/debugger-cli-*/daemon.sock

# Test CLI parsing
cargo run -- --help
cargo run -- break --help
```

---

## Quick Reference

### Key Files for Common Tasks

| Task | Files to Modify |
|------|-----------------|
| Add CLI command | `commands.rs`, `cli/mod.rs` |
| Add IPC command | `ipc/protocol.rs`, `daemon/handler.rs` |
| Add session logic | `daemon/session.rs` |
| Add DAP request | `dap/client.rs`, `dap/types.rs` |
| Add config option | `common/config.rs` |
| Add error type | `common/error.rs` |

### Module Responsibilities

| Module | Responsibility |
|--------|----------------|
| `cli/` | User interaction, command parsing, output formatting |
| `daemon/` | Session state, command handling, DAP orchestration |
| `dap/` | DAP protocol implementation, adapter communication |
| `ipc/` | CLI↔Daemon communication, message serialization |
| `common/` | Shared utilities (config, errors, paths) |

### Session State Machine

```
     ┌─────────────────────────────────────┐
     │                                     │
     ▼                                     │
   Idle ──▶ Initializing ──▶ Configuring ──┼──▶ Running ◀──▶ Stopped
     ▲                                     │        │          │
     │                                     │        ▼          │
     │                                     │     Exited ◀──────┘
     │                                     │        │
     └─────────── Terminating ◀────────────┴────────┘
```

---

Happy debugging! If you have questions, check the existing docs in `docs/plan/` or ask the team.
