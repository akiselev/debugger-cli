# debugger-cli

**A command-line debugger built for LLM coding agents**

[![License: GPL-3.0](https://img.shields.io/badge/License-GPL--3.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

`debugger-cli` is a cross-platform debugging tool that enables LLM coding agents (and humans!) to debug executables using the [Debug Adapter Protocol (DAP)](https://microsoft.github.io/debug-adapter-protocol/). It provides a simple, scriptable CLI interface that maintains persistent debug sessions across multiple command invocations.

## Why This Exists

LLM agents need to debug programs interactively, but CLI commands are ephemeral. Traditional debuggers require an interactive session that's incompatible with agent workflows. This tool solves that by:

- **Maintaining persistent sessions**: A background daemon keeps the debug session alive between commands
- **Buffering events**: Output, breakpoint hits, and stop events are captured even when no client is connected
- **Providing a unified interface**: Works with any DAP adapter (lldb-dap, CodeLLDB, debugpy, Delve, etc.)
- **Being LLM-friendly**: Clear, parseable output optimized for agent consumption

## Features

- **Multi-language support**: Debug C, C++, Rust, Python, Go, and more
- **Zero-friction setup**: `debugger setup lldb` installs everything you need
- **Full breakpoint control**: Line, function, and conditional breakpoints
- **Rich inspection**: Variables, expressions, stack traces, and source context
- **Thread management**: List, switch, and navigate threads and stack frames
- **Structured output**: JSON-friendly for agent consumption
- **Cross-platform**: Linux, macOS, and Windows support

## Quick Start

### Installation

```bash
# Clone and build
git clone https://github.com/akiselev/debugger-cli.git
cd debugger-cli
cargo build --release

# Add to PATH
export PATH="$PWD/target/release:$PATH"

# Install a debug adapter (e.g., lldb for C/C++/Rust)
debugger setup lldb
```

### Prerequisites

You need a DAP-compatible debug adapter. The easiest way is:

```bash
# List available debuggers
debugger setup --list

# Install for your language
debugger setup lldb      # C, C++, Rust
debugger setup python    # Python (debugpy)
debugger setup go        # Go (delve)
```

Or install manually:
- **Arch Linux**: `sudo pacman -S lldb`
- **Ubuntu/Debian**: `sudo apt install lldb`
- **macOS**: `xcode-select --install` (includes lldb)

### Basic Usage

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

## Commands Reference

### Session Management

| Command | Aliases | Description |
|---------|---------|-------------|
| `start <program> [-- args]` | | Start debugging a program |
| `attach <pid>` | | Attach to running process |
| `stop` | | Stop debug session and terminate debuggee |
| `detach` | | Detach from process (keeps it running) |
| `status` | | Show daemon and session status |
| `restart` | | Restart program with same arguments |

### Breakpoints

| Command | Aliases | Description |
|---------|---------|-------------|
| `breakpoint add <location>` | `break`, `b` | Add breakpoint (file:line or function) |
| `breakpoint remove <id>` | | Remove breakpoint by ID |
| `breakpoint remove --all` | | Remove all breakpoints |
| `breakpoint list` | | List all breakpoints |

Breakpoint options:
- `--condition <expr>` - Break only when expression is true
- `--hit-count <n>` - Break after N hits

### Execution Control

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

### Program Output

| Command | Description |
|---------|-------------|
| `output` | Get program stdout/stderr |
| `output --follow` | Stream output continuously |
| `output --tail <n>` | Get last N lines |

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

The tool runs as a **single binary in two modes**:

1. **CLI Mode** (thin client): Parses commands, connects to daemon via IPC, displays results
2. **Daemon Mode** (background): Manages the debug session, communicates with the DAP adapter, buffers events

This architecture allows:
- Persistent debug sessions across multiple CLI invocations
- Event buffering when no client is connected
- Non-blocking command execution
- Clean process lifecycle management

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

## Supported Debug Adapters

| Adapter | Languages | Status |
|---------|-----------|--------|
| lldb-dap | C, C++, Rust, Swift | ✅ Full support |
| CodeLLDB | C, C++, Rust | ✅ Full support |
| debugpy | Python | ✅ Full support |
| Delve | Go | ✅ Full support |
| cpptools | C, C++ | 🚧 Planned |
| js-debug | JavaScript, TypeScript | 🚧 Planned |

## Example: Debugging a Rust Program

```bash
# Build your Rust program with debug info
cargo build

# Start debugging
debugger start ./target/debug/myprogram

# Set breakpoints
debugger break main
debugger break src/lib.rs:42 --condition "x > 100"

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
#    16 |     Ok(())
#    17 | }
#
# Locals:
#   config: Config { max_threads: 4, timeout: 30 }
#   processor: Processor { ... }

# Evaluate expressions
debugger print config.max_threads
# 4

debugger print processor.stats()
# ProcessorStats { processed: 0, errors: 0 }

# Step through code
debugger step
debugger await
debugger context

# Clean up
debugger stop
```

## Development

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for the developer guide, including:
- Architecture deep-dive
- Adding new commands
- Working with the DAP client
- Testing and debugging tips

## License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.

## Contributing

Contributions are welcome! Please feel free to submit issues and pull requests.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request
