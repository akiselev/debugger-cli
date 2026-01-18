# LLM Debugger CLI - Architecture Plan

## Overview

A cross-platform CLI tool that enables LLM coding agents to debug executables using the Debug Adapter Protocol (DAP). The tool uses a single binary with client-daemon architecture to maintain persistent debug sessions across CLI invocations.

## Problem Statement

LLM agents need to debug programs interactively, but CLI commands are ephemeral. The challenge:

1. **DAP adapters (lldb-dap, codelldb) communicate via stdin/stdout** - if the spawning process exits, the pipe closes and the adapter terminates
2. **Socket mode exists but is limited** - lldb-dap supports `--connection tcp://` but it's relatively new and not universally available
3. **Events need buffering** - output events, stopped events, etc. arrive asynchronously and would be lost if no client is connected

Our solution: A **single binary** that runs in two modes:
- **CLI mode**: Parses commands, connects to daemon, displays results
- **Daemon mode**: Spawned automatically, manages DAP adapter and buffers events

## Why Not Just Use the DAP Adapter Directly?

The DAP adapter (lldb-dap) *is* the debugger daemon - it holds all the debug state. However:

| Direct Adapter Connection | Our Daemon Wrapper |
|--------------------------|-------------------|
| Requires socket mode support | Works with any adapter (stdin/stdout) |
| Output events lost when disconnected | Events buffered for later retrieval |
| Different adapters have different connection methods | Unified interface |
| Complex reconnection logic in CLI | Simple IPC protocol |

For maximum compatibility, we wrap the adapter with a thin daemon layer.

## Architecture

```
┌─────────────────┐     IPC (Unix Socket/     ┌─────────────────┐     stdin/stdout    ┌─────────────────┐
│   CLI Mode      │◄───  Named Pipe)      ───►│   Daemon Mode   │◄───   (DAP)     ───►│   DAP Adapter   │
│  (debugger cmd) │                           │(debugger daemon)│                     │  (lldb-dap,     │
│                 │                           │                 │                     │   codelldb)     │
└─────────────────┘                           └─────────────────┘                     └─────────────────┘
        │                                             │                                       │
        │ Same binary, different modes                │                                       ▼
        └─────────────────────────────────────────────┘                               ┌─────────────────┐
                                                                                      │    Debuggee     │
                                                                                      │  (target app)   │
                                                                                      └─────────────────┘
```

### Single Binary, Two Modes

```bash
# CLI mode (user-facing)
debugger start ./myapp
debugger breakpoint add main.rs:10
debugger continue

# Daemon mode (spawned automatically, hidden from user)
debugger daemon  # Started by CLI when needed
```

### Components

1. **CLI Mode**: Thin client that parses commands, spawns daemon if needed, connects via IPC, displays results
2. **Daemon Mode**: Long-running background process that manages DAP adapter subprocess, buffers events, and handles async DAP communication
3. **DAP Adapter**: External debug adapter (lldb-dap, codelldb, etc.) that speaks DAP protocol

## Debug Adapter Protocol (DAP)

### Protocol Basics

DAP uses JSON messages with a simple header format:

```
Content-Length: <byte-length>\r\n
\r\n
<JSON payload>
```

### Message Types

1. **Requests**: Client → Adapter (e.g., `initialize`, `launch`, `setBreakpoints`)
2. **Responses**: Adapter → Client (responses to requests)
3. **Events**: Adapter → Client (asynchronous notifications like `stopped`, `output`)
4. **Reverse Requests**: Adapter → Client (optional, adapter asks client to do something)

### Session Lifecycle

```
Client                              Adapter
  │                                    │
  ├──── initialize ───────────────────►│
  │◄─── initialize response ───────────┤
  │◄─── initialized event ─────────────┤
  │                                    │
  ├──── setBreakpoints ───────────────►│
  │◄─── setBreakpoints response ───────┤
  │                                    │
  ├──── configurationDone ────────────►│
  │◄─── configurationDone response ────┤
  │                                    │
  ├──── launch/attach ────────────────►│
  │◄─── launch/attach response ────────┤
  │                                    │
  │     ... debugging session ...      │
  │                                    │
  ├──── disconnect ───────────────────►│
  │◄─── disconnect response ───────────┤
  │                                    │
```

### Key DAP Requests

| Request | Description |
|---------|-------------|
| `initialize` | Exchange capabilities between client and adapter |
| `launch` | Start debuggee with or without debugging |
| `attach` | Attach to already running process |
| `setBreakpoints` | Set breakpoints for a source file |
| `setFunctionBreakpoints` | Set breakpoints on function names |
| `configurationDone` | Signal end of configuration phase |
| `continue` | Resume execution |
| `next` | Step over (same level) |
| `stepIn` | Step into function |
| `stepOut` | Step out of function |
| `pause` | Pause execution |
| `stackTrace` | Get stack frames for a thread |
| `scopes` | Get variable scopes for a frame |
| `variables` | Get variables in a scope |
| `evaluate` | Evaluate expression in context |
| `threads` | List all threads |
| `disconnect` | End debug session |

### Key DAP Events

| Event | Description |
|-------|-------------|
| `initialized` | Adapter ready for configuration |
| `stopped` | Execution stopped (breakpoint, step, exception) |
| `continued` | Execution resumed |
| `exited` | Debuggee exited with code |
| `terminated` | Debug session ended |
| `output` | Debuggee produced output |
| `thread` | Thread started/exited |
| `breakpoint` | Breakpoint state changed |

## IPC Protocol (CLI ↔ Daemon)

### Transport

Platform-specific IPC transports (selected at compile time via `#[cfg]`):

- **Unix/macOS**: Unix domain sockets at `$XDG_RUNTIME_DIR/debugger-cli/daemon.sock` or `/tmp/debugger-cli-<uid>/daemon.sock`
- **Windows**: Named pipes at `\\.\pipe\debugger-cli-<username>` (completely different transport, not Unix sockets)

The `interprocess` crate's `local_socket` module abstracts this difference, but `paths.rs` must use platform-specific logic to determine the correct path/name.

### Message Format

Simple JSON-RPC style messages:

```json
// Request
{
  "id": 1,
  "command": "breakpoint_add",
  "args": {
    "file": "src/main.rs",
    "line": 42
  }
}

// Response
{
  "id": 1,
  "success": true,
  "result": {
    "breakpoint_id": 1,
    "verified": true
  }
}

// Error Response
{
  "id": 1,
  "success": false,
  "error": {
    "code": "NOT_RUNNING",
    "message": "No debug session active"
  }
}
```

## CLI Commands

### Session Management

```bash
# Start debugging a binary (spawns daemon if needed)
debugger start ./target/debug/myapp [-- args...]
debugger start ./target/debug/myapp --adapter lldb-dap
debugger start ./target/debug/myapp --adapter codelldb

# Attach to running process
debugger attach <pid>
debugger attach <pid> --adapter lldb-dap

# Detach from process (process keeps running)
debugger detach

# Stop debugging (terminates debuggee and daemon)
debugger stop

# Restart program (re-launch with same arguments)
debugger restart

# Check daemon/session status
debugger status
```

### Breakpoints

```bash
# Add breakpoint at line
debugger breakpoint add src/main.rs:42
debugger break src/main.rs:42  # alias

# Add breakpoint at function
debugger breakpoint add --function main
debugger breakpoint add -f "mymodule::MyStruct::method"

# Add conditional breakpoint
debugger breakpoint add src/main.rs:42 --condition "x > 10"

# Add hit-count breakpoint (break after N hits)
debugger breakpoint add src/main.rs:42 --hit-count 5

# List breakpoints
debugger breakpoint list

# Remove breakpoint
debugger breakpoint remove <id>
debugger breakpoint remove --all

# Enable/disable breakpoint
debugger breakpoint enable <id>
debugger breakpoint disable <id>
```

### Watchpoints (Data Breakpoints)

```bash
# Watch variable for writes (break when modified)
debugger watch counter
debugger watch my_struct.field

# Watch for reads
debugger watch counter --read

# Watch for any access (read or write)
debugger watch counter --access

# Watch with condition
debugger watch counter --condition "counter > 100"

# List active watchpoints
debugger watch list

# Remove watchpoint
debugger watch remove <id>
```

### Execution Control

```bash
# Continue execution
debugger continue
debugger c  # alias

# Step over (execute current line, step over function calls)
debugger next
debugger n  # alias

# Step into (execute current line, step into function calls)
debugger step
debugger s  # alias

# Step out (run until current function returns)
debugger finish
debugger out  # alias

# Step to specific line (run until reaching line)
debugger until src/main.rs:50

# Pause execution
debugger pause

# Wait for next stop (breakpoint, step completion, etc.)
debugger await [--timeout 60]
```

### Context & Source View

```bash
# Show current position with source context and variables (THE KEY COMMAND FOR LLMs)
debugger context
debugger where    # alias (note: unrelated to Rust's 'where' keyword; follows GDB/LLDB convention)

# Example output:
# Thread 1 stopped at src/main.rs:42
#
#    40 |     let mut sum = 0;
#    41 |     for i in 0..n {
# -> 42 |         sum += calculate(i);
#    43 |     }
#    44 |     sum
#
# Locals:
# ┌──────────┬─────────┬─────────────────────┐
# │ Name     │ Type    │ Value               │
# ├──────────┼─────────┼─────────────────────┤
# │ n        │ i32     │ 100                 │
# │ sum      │ i32     │ 4950                │
# │ i        │ i32     │ 99                  │
# └──────────┴─────────┴─────────────────────┘

# Show more context lines
debugger where --context 10

# Show source around a specific location (without stopping there)
debugger source src/main.rs:30
debugger list src/main.rs:30  # alias
debugger source src/main.rs:30 --context 20

# Show source for current function
debugger source --function
```

### Stack Navigation

```bash
# Get stack trace
debugger backtrace
debugger bt  # alias

# Backtrace with local variables for each frame
debugger backtrace --locals

# Show only N frames
debugger backtrace --limit 5

# Navigate to specific frame (0 = innermost/current)
debugger frame 2

# Move up/down the stack
debugger up      # go to caller
debugger down    # go back toward current frame

# Show current frame info
debugger frame
```

### Variables & Inspection

```bash
# Get all local variables in current frame
debugger locals

# Get arguments to current function
debugger args

# Print specific variable or expression
debugger print counter
debugger p counter  # alias
debugger print "my_struct.nested.field"
debugger print "*ptr"
debugger print "array[5]"

# Print with type info
debugger print counter --type

# Evaluate expression (can have side effects)
debugger eval "x + y * 2"
debugger eval "vec.push(42)"  # modifies state

# Set/modify variable value
debugger set counter 100
debugger set my_struct.field "new value"

# Show variable in different formats
debugger print counter --format hex
debugger print counter --format binary
debugger print ptr --format address
```

### Threads

```bash
# List all threads
debugger threads

# Example output:
# ┌────┬─────────────┬──────────┬─────────────────────────┐
# │ ID │ Name        │ State    │ Location                │
# ├────┼─────────────┼──────────┼─────────────────────────┤
# │ 1  │ main        │ stopped  │ src/main.rs:42          │
# │ 2  │ worker-1    │ running  │ src/worker.rs:15        │
# │ 3  │ worker-2    │ stopped  │ src/worker.rs:28        │
# └────┴─────────────┴──────────┴─────────────────────────┘

# Switch to specific thread
debugger thread 2

# Show current thread info
debugger thread

# Continue only current thread (others stay stopped)
debugger continue --thread
```

### Memory & Registers

```bash
# Read memory at address
debugger memory read 0x7ffc12345678
debugger memory read 0x7ffc12345678 --count 64
debugger memory read 0x7ffc12345678 --format hex

# Read memory at variable address
debugger memory read &my_var --count 32

# Read registers
debugger registers

# Read specific register
debugger registers rax rsp
```

### Exception Handling

```bash
# Break on exceptions/panics
debugger catch panic
debugger catch throw      # C++ exceptions

# Break on specific exception type
debugger catch throw std::runtime_error

# List exception catchpoints
debugger catch list

# Remove exception catchpoint
debugger catch remove <id>
```

### Output & Logging

```bash
# Get debuggee stdout/stderr output
debugger output

# Stream output continuously (follows)
debugger output --follow

# Get last N lines of output
debugger output --tail 50

# Clear output buffer
debugger output --clear
```

### Disassembly

```bash
# Disassemble current function
debugger disassemble

# Disassemble at address
debugger disassemble 0x555555554000

# Disassemble N instructions
debugger disassemble --count 20

# Mixed source and assembly
debugger disassemble --source
```

### LLDB/GDB Raw Commands

```bash
# Send raw command to underlying debugger (escape hatch)
debugger raw "thread backtrace all"
debugger raw "memory read --size 4 --count 10 0x1000"
```

## Daemon State Machine

```
                           ┌─────────────────┐
                           │      IDLE       │
                           │  (no session)   │
                           └────────┬────────┘
                                    │ start/attach
                                    ▼
                           ┌─────────────────┐
                           │  INITIALIZING   │
                           │ (starting DAP)  │
                           └────────┬────────┘
                                    │ initialized
                                    ▼
                           ┌─────────────────┐
                           │   CONFIGURING   │
                           │(set breakpoints)│
                           └────────┬────────┘
                                    │ configurationDone
                                    ▼
              ┌────────────────────────────────────────┐
              │                                        │
              ▼                                        │
     ┌─────────────────┐                     ┌─────────────────┐
     │     RUNNING     │◄───── continue ─────│     STOPPED     │
     │                 │────── stopped ─────►│                 │
     └─────────────────┘                     └─────────────────┘
              │                                        │
              │ exited/terminated                      │
              ▼                                        │
     ┌─────────────────┐                              │
     │    TERMINATED   │◄─────────────────────────────┘
     │                 │        disconnect
     └────────┬────────┘
              │ cleanup
              ▼
     ┌─────────────────┐
     │      IDLE       │
     └─────────────────┘
```

## Daemon Lifecycle & Session Management

### Single Session Design

The daemon supports **one debug session at a time**. This simplifies:
- State management (no session isolation needed)
- Resource usage (single DAP adapter process)
- CLI commands (no session selection required)

To debug multiple programs simultaneously, run separate daemon instances (future enhancement).

### Daemon Lifecycle

```
1. First CLI command runs (e.g., `debugger start ./app`)
2. CLI checks for existing daemon socket
3. If no daemon: CLI spawns `debugger daemon` in background
4. Daemon creates IPC socket and waits for connections
5. CLI connects and sends command
6. Daemon processes commands until `stop` or session ends
7. Daemon remains running (IDLE state) for potential new sessions
8. Daemon auto-exits after configurable idle timeout (default: 30 minutes)
```

### Crash Recovery

If the daemon crashes:
- CLI detects connection failure on next command
- CLI spawns a new daemon automatically
- User must re-establish debug session (`debugger start`)
- Output buffer from previous session is lost

If the DAP adapter crashes:
- Daemon receives EOF on adapter stdout
- Daemon transitions to TERMINATED → IDLE state
- Next CLI command reports "Session terminated unexpectedly"
- User can start a new session

### Idle Timeout

To prevent orphaned daemons:
```toml
# ~/.config/debugger-cli/config.toml
[daemon]
idle_timeout_minutes = 30  # Auto-exit after 30 min with no session
```

## Timeout Configuration

### Default Timeouts

| Operation | Default | Configurable |
|-----------|---------|--------------|
| Daemon spawn wait | 5 seconds | No |
| IPC connect | 2 seconds | No |
| DAP initialize | 10 seconds | Yes |
| DAP request (general) | 30 seconds | Yes |
| `await` command | 300 seconds | Yes (per-command) |
| Daemon idle | 30 minutes | Yes |

### Configuration

```toml
# ~/.config/debugger-cli/config.toml
[timeouts]
dap_initialize_secs = 10
dap_request_secs = 30
await_default_secs = 300

[daemon]
idle_timeout_minutes = 30
```

### Per-Command Overrides

```bash
# Override await timeout
debugger await --timeout 120

# Most commands use default timeout, not overridable
debugger continue  # Uses dap_request_secs
```

## Output Buffer Management

### Buffer Design

The daemon buffers debuggee output (stdout/stderr) when no CLI client is connected:

```rust
pub struct OutputBuffer {
    events: VecDeque<OutputEvent>,
    max_size: usize,        // Default: 10,000 events
    max_bytes: usize,       // Default: 10 MB
    current_bytes: usize,
}
```

### Buffer Limits

| Limit | Default | Purpose |
|-------|---------|---------|
| Max events | 10,000 | Prevent memory exhaustion |
| Max bytes | 10 MB | Hard cap on memory usage |

When buffer is full, oldest events are dropped (circular buffer behavior).

### Configuration

```toml
# ~/.config/debugger-cli/config.toml
[output]
max_events = 10000
max_bytes_mb = 10
```

### Retrieval

```bash
# Get buffered output (clears buffer)
debugger output

# Get last N lines only
debugger output --tail 100

# Stream new output (doesn't clear buffer)
debugger output --follow
```

## Crate Dependencies

### Core Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` | CLI argument parsing |
| `serde` / `serde_json` | JSON serialization |
| `tokio` | Async runtime |
| `interprocess` | Cross-platform IPC (Unix sockets / Named pipes) |
| `thiserror` | Error handling |
| `tracing` / `tracing-subscriber` | Logging |

### DAP Protocol

| Crate | Purpose |
|-------|---------|
| `dap-types` | DAP type definitions (we extend for client use) |

Note: Existing DAP crates (`dap-rs`, `dap-types`) focus on server implementation. We'll need to build the client ourselves using the type definitions.

### Optional Dependencies

| Crate | Purpose |
|-------|---------|
| `which` | Find debug adapter binaries in PATH |
| `directories` | Platform-specific directories |
| `ctrlc` | Graceful shutdown handling |
| `crossterm` | Terminal formatting for output |

## Project Structure

Single crate with modular organization:

```
debugger-cli/
├── Cargo.toml
├── docs/
│   └── plan/
│       ├── architecture.md (this file)
│       ├── implementation.md
│       └── dap-reference.md
├── src/
│   ├── main.rs               # Entry point, CLI parsing
│   ├── cli/                  # CLI command handlers
│   │   ├── mod.rs
│   │   ├── start.rs
│   │   ├── breakpoint.rs
│   │   ├── execution.rs      # continue, step, next, etc.
│   │   ├── inspect.rs        # backtrace, print, locals
│   │   └── session.rs        # stop, detach, status
│   ├── daemon/               # Daemon mode
│   │   ├── mod.rs
│   │   ├── server.rs         # IPC server loop
│   │   ├── session.rs        # Debug session state machine
│   │   └── handler.rs        # Command handlers
│   ├── dap/                  # DAP protocol implementation
│   │   ├── mod.rs
│   │   ├── types.rs          # DAP message types
│   │   ├── codec.rs          # Wire protocol (Content-Length framing)
│   │   └── client.rs         # DAP client (sends requests, receives responses/events)
│   ├── ipc/                  # CLI ↔ Daemon communication
│   │   ├── mod.rs
│   │   ├── protocol.rs       # Request/Response types
│   │   ├── client.rs         # CLI-side connection
│   │   └── transport.rs      # Cross-platform socket/pipe
│   └── common/               # Shared utilities
│       ├── mod.rs
│       ├── paths.rs          # Socket paths, config paths
│       ├── config.rs         # Configuration file handling
│       └── error.rs          # Error types
└── tests/
    ├── integration/
    │   └── basic_session.rs
    └── mock_adapter/         # Fake DAP adapter for testing
        └── main.rs
```

## Debug Adapter Support

### Primary: lldb-dap

- Part of LLVM project
- Supports C, C++, Rust, Swift, Objective-C
- Available via: `lldb-dap` binary, `xcrun lldb-dap` on macOS

### Secondary: CodeLLDB

- VS Code extension with standalone adapter
- Better Rust support (formatters, visualizers)
- Downloadable from GitHub releases

### Configuration

```toml
# ~/.config/debugger-cli/config.toml

[adapters.lldb-dap]
path = "lldb-dap"  # or full path
args = []

[adapters.codelldb]
path = "~/.local/share/debugger-cli/adapters/codelldb/adapter/codelldb"
args = []

[defaults]
adapter = "lldb-dap"
```

## Error Handling Strategy

1. **CLI Errors**: Invalid arguments, connection failures → immediate exit with message
2. **Daemon Errors**: Log and try to recover, notify CLI of failures
3. **DAP Errors**: Parse adapter responses, translate to user-friendly messages
4. **Timeout Handling**: All async operations have configurable timeouts

## Security Considerations

1. **IPC Socket/Pipe Permissions**:
   - **Unix/macOS**: Create socket directory with mode `0700` and socket with mode `0600`, ensuring only the owning user can access
   - **Windows**: Create named pipe with a security descriptor (DACL) that restricts access to the current user. When using `interprocess` or raw Win32 APIs, set `SECURITY_ATTRIBUTES` with an owner-only ACL rather than relying on default permissions

2. **Adapter Validation**: Validate adapter binary exists and is executable before spawning
3. **No Arbitrary Code Execution**: Only allow debugger commands, not shell injection

## Testing Strategy

1. **Unit Tests**: Protocol parsing, command parsing, state machine logic
2. **Integration Tests**: Spawn actual daemon, test command flows
3. **Mock Adapter**: Fake DAP adapter for testing without real debugger

## References

- [Debug Adapter Protocol Specification](https://microsoft.github.io/debug-adapter-protocol/specification)
- [DAP Overview](https://microsoft.github.io/debug-adapter-protocol/overview.html)
- [lldb-dap Documentation](https://lldb.llvm.org/use/lldbdap.html)
- [interprocess crate](https://docs.rs/interprocess/latest/interprocess/)
- [dap-types crate](https://github.com/lapce/dap-types)
