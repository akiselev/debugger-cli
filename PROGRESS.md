# Project Status & Roadmap

This document tracks the implementation status of debugger-cli.

> **Current Version**: 0.1.0  
> **Status**: Feature-complete for core debugging workflows  
> **Last Updated**: 2026-01-20

## Implementation Status

### ✅ Completed

| Feature | Description |
|---------|-------------|
| **Session Management** | start, attach, stop, detach, status |
| **Breakpoints** | add, remove, list, conditional, hit-count |
| **Execution Control** | continue, next, step, finish, pause, await |
| **Inspection** | context, locals, backtrace, print, eval |
| **Thread/Frame Navigation** | threads, thread, frame, up, down |
| **Output Capture** | output command with buffering |
| **Setup Command** | Installer for debug adapters |
| **Configuration** | TOML config file support |
| **Cross-platform IPC** | Unix sockets, Windows named pipes |
| **DAP Protocol** | Full client implementation |

### 🚧 In Progress

| Feature | Description | Notes |
|---------|-------------|-------|
| `restart` command | Restart program with same args | Stub implemented |
| Breakpoint enable/disable | Toggle breakpoints | Commands defined |

### 📋 Planned

| Feature | Description | Priority |
|---------|-------------|----------|
| Watchpoints | Data breakpoints on variable changes | Medium |
| Memory read | Raw memory inspection | Low |
| Source listing | List source without stopping | Low |
| Disassembly | View assembly at location | Low |
| Register inspection | CPU register values | Low |
| Core dump debugging | Debug from core files | Low |

## Command Reference

| Command | Aliases | Status | Description |
|---------|---------|--------|-------------|
| `start <program>` | | ✅ | Start debugging a program |
| `attach <pid>` | | ✅ | Attach to running process |
| `stop` | | ✅ | Stop debug session |
| `detach` | | ✅ | Detach (keep process running) |
| `status` | | ✅ | Show daemon/session status |
| `breakpoint add` | `break`, `b` | ✅ | Add breakpoint |
| `breakpoint remove` | | ✅ | Remove breakpoint |
| `breakpoint list` | | ✅ | List all breakpoints |
| `continue` | `c` | ✅ | Continue execution |
| `next` | `n` | ✅ | Step over |
| `step` | `s` | ✅ | Step into |
| `finish` | `out` | ✅ | Step out |
| `pause` | | ✅ | Pause execution |
| `await` | | ✅ | Wait for stop event |
| `backtrace` | `bt` | ✅ | Show stack trace |
| `locals` | | ✅ | Show local variables |
| `print <expr>` | `p` | ✅ | Evaluate expression |
| `eval <expr>` | | ✅ | Evaluate with side effects |
| `context` | `where` | ✅ | Source context + variables |
| `threads` | | ✅ | List threads |
| `thread <id>` | | ✅ | Switch thread |
| `frame <n>` | | ✅ | Switch frame |
| `up` | | ✅ | Move up stack |
| `down` | | ✅ | Move down stack |
| `output` | | ✅ | Get program output |
| `restart` | | 🚧 | Restart program |
| `setup` | | ✅ | Install debug adapters |
| `test` | | ✅ | Run YAML test scenarios |
| `logs` | | ✅ | View daemon logs |

## Supported Debug Adapters

| Adapter | Languages | Install Method | Status |
|---------|-----------|----------------|--------|
| lldb-dap | C, C++, Rust, Swift | System package / LLVM | ✅ Full |
| CodeLLDB | C, C++, Rust | GitHub releases | ✅ Full |
| debugpy | Python | pip install | ✅ Full |
| Delve | Go | go install / releases | ✅ Full |
| cpptools | C, C++ | VS Code extension | 📋 Planned |
| js-debug | JavaScript, TypeScript | VS Code extension | 📋 Planned |

## Quick Test

```bash
# If installed via cargo install
debugger --help
debugger status

# If building from source
cargo build --release
./target/release/debugger --help
./target/release/debugger status
```

## Full Test (requires lldb-dap)

```bash
# Create test program
cat > /tmp/test.c << 'EOF'
#include <stdio.h>
int main() {
    int x = 42;
    printf("x = %d\n", x);
    return 0;
}
EOF
gcc -g -o /tmp/test /tmp/test.c

# Debug it
./target/release/debugger start /tmp/test --stop-on-entry
./target/release/debugger breakpoint add main
./target/release/debugger continue
./target/release/debugger await
./target/release/debugger context
./target/release/debugger stop
```
