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

- **Unix/macOS**: Unix domain sockets at `$XDG_RUNTIME_DIR/debugger-cli/daemon.sock` or `/tmp/debugger-cli-<uid>/daemon.sock`
- **Windows**: Named pipes at `\\.\pipe\debugger-cli-<username>`

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

# List breakpoints
debugger breakpoint list

# Remove breakpoint
debugger breakpoint remove <id>
debugger breakpoint remove --all

# Enable/disable breakpoint
debugger breakpoint enable <id>
debugger breakpoint disable <id>
```

### Execution Control

```bash
# Continue execution
debugger continue
debugger c  # alias

# Step over
debugger next
debugger n  # alias

# Step into
debugger step
debugger s  # alias

# Step out
debugger finish
debugger out  # alias

# Pause execution
debugger pause

# Wait for next stop (breakpoint, step completion, etc.)
debugger await [--timeout 60]
```

### Inspection

```bash
# Get stack trace
debugger backtrace
debugger bt  # alias

# List threads
debugger threads

# Switch thread
debugger thread <id>

# Get local variables
debugger locals

# Get specific variable
debugger print <expr>
debugger p <expr>  # alias

# Evaluate expression
debugger eval "<expression>"

# Watch expression (data breakpoint)
debugger watch <expr>
debugger watch <expr> --read   # break on read
debugger watch <expr> --write  # break on write (default)
debugger watch <expr> --access # break on read or write
```

### Memory & Registers

```bash
# Read memory
debugger memory read <address> [--count 64]

# Read registers
debugger registers
```

### Output

```bash
# Get debuggee stdout/stderr output
debugger output

# Stream output continuously
debugger output --follow
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

1. **IPC Socket Permissions**: Restrict socket/pipe to current user only (0700 directory, 0600 socket)
2. **Adapter Validation**: Validate adapter binary exists and is executable
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
