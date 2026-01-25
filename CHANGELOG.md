# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-01-25

### Added

- **Delve (Go) Support**: Full Go debugging via Delve DAP
  - `setup go` / `setup delve` - Install and configure Delve adapter
  - TCP transport mode for Delve's DAP server
  - Go project detection via `go.mod` / `go.sum`
  - `mode: "exec"` for pre-compiled binaries
  - Delve-specific `stopAtEntry` handling

- **GDB Support**: Native DAP support for GDB 14.1+
  - `setup gdb` - Install and configure GDB adapter
  - Uses `-i=dap` interpreter mode for direct DAP communication
  - Version detection and validation (requires GDB ≥14.1)

- **CUDA-GDB Support**: NVIDIA GPU debugging via cuda-gdb
  - `setup cuda-gdb` - Install and configure CUDA-GDB adapter
  - **Dual-mode architecture**: Automatically detects best mode
    - Native DAP (`-i=dap`) for NVIDIA official installs with DAP support
    - cdt-gdb-adapter bridge for minimal builds (e.g., Arch Linux)
  - CUDA project detection via `*.cu` files
  - Linux-only (NVIDIA driver limitation)

- **Initial Breakpoints**: Set breakpoints before program starts
  - `--break` / `-b` flag for `start` command
  - Set multiple breakpoints: `debugger start ./prog --break main --break file.c:42`
  - Essential for adapters that don't support `stopOnEntry` (e.g., cdt-gdb-adapter)
  - Breakpoints set during DAP configuration phase (before `configurationDone`)

- **Adapter-specific Stop-on-Entry**: Proper handling for different adapters
  - GDB/CUDA-GDB: `stopAtBeginningOfMainSubprogram`
  - Delve: `stopAtEntry`
  - Others: `stopOnEntry`

### Fixed

- **cuda-gdb Version Parsing**: Handle cuda-gdb's "exec:" wrapper line in version output
  - Parser now searches for "GNU gdb X.Y" pattern across all lines
  - Correctly extracts base GDB version (14.2) instead of cuda-gdb version (13.1)

- **Address Parsing**: Enhanced address extraction in DAP client and verifier

### Documentation

- Added `docs/plan/cuda-gdb.md` with architecture details and tested features
- Added `docs/plan/go-delve-support.md` with Go debugging guide
- Updated `src/setup/adapters/CLAUDE.md` with adapter-specific behaviors
- Added `src/setup/adapters/README.md` with usage examples

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
