# debugger-cli

[![Crates.io](https://img.shields.io/crates/v/debugger-cli.svg)](https://crates.io/crates/debugger-cli)
[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

> **Experimental.** Commands, adapter behavior, and output schemas may change while the debugger interface is still being developed.

`debugger-cli` is a cross-platform, scriptable debugger built on the [Debug Adapter Protocol (DAP)](https://microsoft.github.io/debug-adapter-protocol/). A background daemon keeps debug sessions alive across separate CLI invocations and buffers debugger events between commands.

## Design

The CLI is intended to make interactive debugger state accessible to ordinary shell automation:

- **Persistent sessions**: a background daemon keeps the debug session alive between commands
- **Buffered events**: output, breakpoint hits, and stop events are retained when no client is connected
- **DAP adapters**: the same CLI can drive lldb-dap, CodeLLDB, debugpy, Delve, GDB, and other compatible adapters
- **Structured output**: commands expose parseable output for scripts and other tools

## Features

- Debug C, C++, Rust, Python, Go, and other languages supported by installed DAP adapters
- Install supported adapters with `debugger setup`
- Line, function, conditional, and hit-count breakpoints
- Variables, expressions, stack traces, source context, threads, and frames
- Structured command output
- Linux, macOS, and Windows support

## Quick start

### Installation

```bash
# Install from crates.io
cargo install debugger-cli

# Install a debug adapter (e.g. lldb for C/C++/Rust)
debugger setup lldb
```

Or build from source:

```bash
git clone https://github.com/akiselev/debugger-cli.git
cd debugger-cli
cargo install --path .
```

### Prerequisites

You need a DAP-compatible debug adapter. The setup command can install supported adapters:

```bash
# List available debuggers
debugger setup --list

# Install for your language
debugger setup lldb      # C, C++, Rust, Swift
debugger setup python    # Python (debugpy)
debugger setup go        # Go (Delve)
debugger setup gdb       # C, C++ (requires GDB 14.1+)
debugger setup cuda-gdb  # CUDA (Linux only)
```

Or install one manually:

- **Arch Linux**: `sudo pacman -S lldb`
- **Ubuntu/Debian**: `sudo apt install lldb`
- **macOS**: `xcode-select --install` (includes lldb)

### Basic usage

```bash
# Start debugging a program
debugger start ./myprogram

# Set a breakpoint
debugger break main.c:42
# or
debugger breakpoint add my_function

# Run until breakpoint
debugger continue

# Wait for the program to stop
debugger await

# Inspect current state
debugger context        # Source code + variables
debugger locals         # Local variables
debugger backtrace      # Stack trace
debugger print myvar    # Evaluate expression

# Step through code
debugger next           # Step over
debugger step           # Step into
debugger finish         # Step out

# Clean up
debugger stop
```

## Commands reference

### Session management

| Command | Aliases | Description |
|---------|---------|-------------|
| `start <program> [-- args]` | | Start debugging a program |
| `attach <pid>` | | Attach to running process |
| `stop` | | Stop debug session and terminate debuggee |
| `detach` | | Detach from process (keeps it running) |
| `status` | | Show daemon and session status |
| `restart` | | Restart program when supported by the active DAP adapter |

Start options:

- `--adapter <name>`: use a specific debug adapter
- `--stop-on-entry`: stop at program entry point
- `--break <location>` / `-b`: set initial breakpoint(s) before program starts

### Breakpoints

| Command | Aliases | Description |
|---------|---------|-------------|
| `breakpoint add <location>` | `break`, `b` | Add breakpoint (file:line or function) |
| `breakpoint remove <id>` | | Remove breakpoint by ID |
| `breakpoint remove --all` | | Remove all breakpoints |
| `breakpoint list` | | List all breakpoints |
| `breakpoint enable <id>` | | Enable a disabled breakpoint |
| `breakpoint disable <id>` | | Disable a breakpoint without removing it |

Breakpoint options:

- `--condition <expr>`: break only when expression is true
- `--hit-count <n>`: break after N hits

### Execution control

| Command | Aliases | Description |
|---------|---------|-------------|
| `continue` | `c` | Resume execution |
| `next` | `n` | Step over (execute current line) |
| `step` | `s` | Step into (enter function calls) |
| `finish` | `out` | Step out (run until function returns) |
| `pause` | | Pause execution |
| `await` | | Wait for next stop event |

### Inspection

| Command | Aliases | Description |
|---------|---------|-------------|
| `context` | `where` | Show source + variables at current position |
| `locals` | | Show local variables |
| `backtrace` | `bt` | Show stack trace |
| `print <expr>` | `p` | Evaluate expression |
| `eval <expr>` | | Evaluate with side effects |
| `threads` | | List all threads |

### Navigation

| Command | Description |
|---------|-------------|
| `thread <id>` | Switch to thread |
| `frame <n>` | Navigate to stack frame |
| `up` | Move up the stack (to caller) |
| `down` | Move down the stack |

### Program output

| Command | Description |
|---------|-------------|
| `output` | Get program stdout/stderr |
| `output --follow` | Stream output continuously |
| `output --tail <n>` | Get last N lines |
| `output --clear` | Print and clear buffered output |

### Setup

| Command | Description |
|---------|-------------|
| `setup <debugger>` | Install a debug adapter |
| `setup --list` | List available debuggers |
| `setup --check` | Check installed debuggers |
| `setup --auto` | Auto-install for detected project |

## Architecture

```
┌─────────────────┐     IPC Socket      ┌─────────────────┐     stdio (DAP)    ┌─────────────────┐
│   CLI Mode      │◄──────────────────►│   Daemon Mode   │◄──────────────────►│   DAP Adapter   │
│  (user facing)  │                     │  (background)   │                     │   (lldb-dap)    │
└─────────────────┘                     └─────────────────┘                     └─────────────────┘
```

The tool runs as a single binary in two modes:

1. **CLI mode**: parses commands, connects to the daemon over IPC, and displays results
2. **Daemon mode**: manages the debug session, communicates with the DAP adapter, and buffers events

This provides persistent sessions, event buffering, non-blocking commands, and explicit process lifecycle management.

## Configuration

Configuration is stored in `~/.config/debugger-cli/config.toml`:

```toml
# Default debug adapter
adapter = "lldb-dap"

# Request timeout in seconds
timeout = 30

# Custom adapter paths
[adapters]
lldb-dap = "/usr/bin/lldb-dap"
codelldb = "~/.local/share/debugger-cli/adapters/codelldb/adapter/codelldb"
```

## Supported debug adapters

| Adapter | Languages | Status |
|---------|-----------|--------|
| lldb-dap | C, C++, Rust, Swift | Full support |
| debugpy | Python | Full support |
| Delve | Go | Full support |
| GDB | C, C++ | Full support (requires GDB 14.1+) |
| CUDA-GDB | CUDA, C, C++ | Full support (Linux only) |
| js-debug | JavaScript, TypeScript | Full support |
| CodeLLDB | C, C++, Rust | Full support |
| cpptools | C, C++ | Planned |

## Examples

### Debugging a Rust program

```bash
# Build with debug info
cargo build

# Start debugging with initial breakpoint
debugger start ./target/debug/myprogram --break main

# Run to breakpoint
debugger continue
debugger await

# Inspect state
debugger context
# Thread 1 stopped at src/main.rs:15
#
#    13 |     let config = Config::load()?;
#    14 |     let processor = Processor::new(config);
# -> 15 |     processor.run()?;
#
# Locals:
#   config: Config { max_threads: 4, timeout: 30 }

# Evaluate expressions
debugger print config.max_threads
# 4

# Step through code
debugger step
debugger await

# Clean up
debugger stop
```

### Debugging a Go program

```bash
# Build with debug info
go build -gcflags="all=-N -l" -o myprogram

# Start debugging
debugger start ./myprogram --adapter go --break main.main

# Continue to breakpoint
debugger continue
debugger await

# Inspect goroutines
debugger threads

# View locals
debugger locals

# Clean up
debugger stop
```

### Debugging CUDA code (Linux)

```bash
# Compile with debug info
nvcc -g -G -o cuda_program kernel.cu

# Start debugging (uses cuda-gdb)
debugger start ./cuda_program --adapter cuda-gdb --break main

# Set kernel breakpoint
debugger break vectorAdd

# Run to kernel
debugger continue
debugger await

# View CUDA threads
debugger threads

debugger stop
```

## Development

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the developer guide, including:

- architecture details
- adding new commands
- working with the DAP client
- testing and debugging

## License

This project is licensed under the GNU General Public License v3.0. See [LICENSE](LICENSE) for details.

## Contributing

Contributions are welcome.

1. Fork the repository.
2. Create a feature branch.
3. Commit the change.
4. Push the branch.
5. Open a pull request.
