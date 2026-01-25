# GDB and CUDA-GDB Support Implementation Plan

## Overview

This plan adds DAP support for GDB and CUDA-GDB debuggers to debugger-cli.

### Key Discoveries (2026-01-24)

1. **CUDA-GDB Native DAP Support Varies by Distribution**:
   - NVIDIA official installs (Ubuntu via `cuda-gdb-13-1` package): Native DAP works via `-i=dap`
   - Arch Linux `cuda` package: Minimal build without DAP Python bindings ("Interpreter `dap' unrecognized")

2. **Dual-Mode Architecture**: The adapter now supports two modes:
   - **Native DAP** (preferred): `cuda-gdb -i=dap` when available
   - **cdt-gdb-adapter bridge** (fallback): Node.js-based MI-to-DAP bridge for minimal builds

3. **Stop-on-Entry Behavior**:
   - Native DAP (GDB 14.1+): Uses `stopAtBeginningOfMainSubprogram` (not `stopOnEntry`)
   - cdt-gdb-adapter: Does NOT support stop-on-entry; use a breakpoint on `main` instead

### Architecture

```
Native DAP Mode (preferred):
  Client <-> cuda-gdb -i=dap <-> GPU

Bridge Mode (fallback for minimal builds):
  Client <-> cdt-gdb-adapter (Node.js) <-> cuda-gdb (MI mode) <-> GPU
```

### Requirements

| Mode | Requirements |
|------|--------------|
| Native DAP | cuda-gdb based on GDB 14.1+ with DAP Python bindings |
| Bridge (fallback) | cuda-gdb (any version) + Node.js + cdt-gdb-adapter (`npm install -g cdt-gdb-adapter`) |

### Mode Detection

The adapter automatically detects the best mode:
1. Check if cuda-gdb has GDB 14.1+ base version
2. Test `-i=dap -batch -ex quit` for "Interpreter `dap' unrecognized" error
3. If native DAP works → use it; otherwise → use cdt-gdb-adapter bridge

## Adapter-Specific Behaviors

### Stop-on-Entry

| Adapter | Stop-on-Entry Support | Parameter |
|---------|----------------------|-----------|
| lldb-dap | ✅ | `stopOnEntry: true` |
| GDB native DAP | ✅ | `stopAtBeginningOfMainSubprogram: true` |
| Delve (Go) | ✅ | `stopAtEntry: true` |
| debugpy | ✅ | `stopOnEntry: true` |
| cdt-gdb-adapter | ❌ | Use `--break main` instead |

### Using --break for Initial Breakpoints

For adapters that don't support stop-on-entry (like cdt-gdb-adapter), or when you want to stop at a specific location:

```bash
# Stop at main function
debugger start ./program --break main

# Stop at specific line
debugger start ./program --break src/main.cu:42

# Multiple breakpoints
debugger start ./program --break main --break vectorAdd
```

Initial breakpoints are set during the DAP configuration phase (between `initialized` event and `configurationDone`), ensuring they're active before program execution begins.

## Version Parsing

cuda-gdb outputs a wrapper message on the first line which broke simple version parsing:

```
exec: /opt/cuda/bin/cuda-gdb
NVIDIA (R) CUDA Debugger
13.1 release
...
GNU gdb (GDB) 14.2
```

The parser now searches for "GNU gdb X.Y" pattern to extract the base GDB version (14.2), ignoring the cuda-gdb version (13.1).

## Tested Features

Tested on Lambda Labs VM (A10 GPU, cuda-gdb 13.1 with native DAP):

| Feature | Status |
|---------|--------|
| Stop at entry point | ✅ via `stopAtBeginningOfMainSubprogram` |
| Backtrace | ✅ Shows source location |
| Local variables | ✅ |
| Function breakpoint on kernel | ✅ |
| CUDA thread visibility | ✅ Shows GPU threads (e.g., `cuda00001400006`) |
| Continue to completion | ✅ |

Tested on Arch Linux (cuda-gdb 13.1 minimal, via cdt-gdb-adapter bridge):

| Feature | Status |
|---------|--------|
| DAP initialize | ✅ |
| Function breakpoint | ✅ verified=True |
| Hit breakpoint | ✅ reason="function breakpoint" |
| Stack trace | ✅ |
| Scopes (Locals/Registers) | ✅ |
| Continue to completion | ✅ |
| Stop-on-entry | ❌ (use `--break main` instead) |

## Code Changes Summary

### Files Modified

| File | Change |
|------|--------|
| `src/setup/adapters/gdb_common.rs` | Fixed version parser to handle cuda-gdb's "exec:" wrapper |
| `src/setup/adapters/cuda_gdb.rs` | Added native DAP detection + cdt-gdb-adapter fallback |
| `src/dap/types.rs` | Added `stopAtBeginningOfMainSubprogram` field |
| `src/daemon/session.rs` | Set GDB-specific stop flag for gdb/cuda-gdb adapters |

### Key Functions

**`has_native_dap_support(cuda_gdb_path)`** in `cuda_gdb.rs`:
```rust
// 1. Check version >= 14.1
// 2. Run: cuda-gdb -i=dap -batch -ex quit
// 3. Check stderr for "Interpreter `dap' unrecognized"
// 4. Return true if no error (native DAP available)
```

**`parse_gdb_version(output)`** in `gdb_common.rs`:
```rust
// Skip "exec:" wrapper lines
// Search for "GNU gdb X.Y" pattern
// Return X.Y as version string
```

## Decision Log

| Decision | Reasoning |
|----------|-----------|
| Native DAP preferred over bridge | Zero dependencies, direct control, better performance |
| Fallback to cdt-gdb-adapter | Arch Linux and similar minimal builds lack DAP Python bindings |
| Auto-detect mode at setup time | User doesn't need to know which mode is available |
| Use `stopAtBeginningOfMainSubprogram` for GDB | GDB's DAP implementation uses this parameter, not `stopOnEntry` |
| Version check extracts GDB base version | cuda-gdb version (13.1) differs from GDB base (14.2) |

## Known Limitations

1. **cdt-gdb-adapter stop-on-entry**: Not supported. Use `--break main` as workaround.
2. **GPU compute capability**: CUDA 13.1 requires sm_75+ (Turing or newer). Older GPUs cannot run CUDA code.
3. **Kernel debugging context**: Breakpoints in kernels may show CPU-side context during `cudaDeviceSynchronize`.
