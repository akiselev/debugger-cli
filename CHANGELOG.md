# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-01-18

### Added

- **Core Debugging**: Full debug session lifecycle management
  - `start` - Launch program for debugging
  - `attach` - Attach to running process
  - `stop` - Stop debug session and terminate debuggee
  - `detach` - Detach from process (keeps it running)
  - `restart` - Restart program with same arguments (stub)

- **Breakpoint Management**
  - `breakpoint add` - Add breakpoints by file:line or function name
  - `breakpoint remove` - Remove breakpoints by ID or all
  - `breakpoint list` - List all breakpoints
  - Conditional breakpoints with `--condition`
  - Hit count breakpoints with `--hit-count`

- **Execution Control**
  - `continue` - Resume execution
  - `next` - Step over
  - `step` - Step into
  - `finish` - Step out
  - `pause` - Pause execution
  - `await` - Wait for next stop event

- **Inspection**
  - `context` / `where` - Show source code + variables at current position
  - `locals` - Display local variables
  - `backtrace` - Show stack trace
  - `print` - Evaluate expressions
  - `eval` - Evaluate expressions with side effects
  - `threads` - List all threads

- **Navigation**
  - `thread` - Switch to specific thread
  - `frame` - Navigate to stack frame
  - `up` / `down` - Move through stack frames

- **Program Output**
  - `output` - Get program stdout/stderr
  - Output buffering in daemon for async capture
  - `--follow` flag for streaming output

- **Debug Adapter Setup**
  - `setup` command for installing debug adapters
  - Support for lldb-dap, CodeLLDB, debugpy, Delve
  - Auto-detection of project types

- **Architecture**
  - Client-daemon architecture for persistent sessions
  - Event buffering when client disconnected
  - Cross-platform IPC (Unix sockets / Windows named pipes)
  - Full DAP protocol implementation

- **Developer Experience**
  - TOML configuration file support
  - Verbose daemon logging
  - LLM-friendly error messages
  - Comprehensive CLI help

### Fixed

- DAP initialization sequence now correctly follows protocol spec:
  `initialize` → `launch/attach` → wait for `initialized` event → `configurationDone`
- `locals` and `print` commands auto-fetch stack frame if needed
