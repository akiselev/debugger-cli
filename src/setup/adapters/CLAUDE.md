# Debug Adapter Installers

| File | What | When |
|------|------|------|
| codelldb.rs | CodeLLDB installer (VS Code LLDB extension) | Setting up Rust/C/C++ debugging |
| cuda_gdb.rs | CUDA-GDB installer for NVIDIA GPU debugging | Setting up CUDA project debugging on Linux |
| debugpy.rs | Python debugger installer | Setting up Python debugging |
| delve.rs | Go debugger installer | Setting up Go debugging |
| gdb.rs | GDB native DAP adapter installer | Setting up C/C++ debugging with GDB ≥14.1 |
| gdb_common.rs | Shared utilities for GDB and CUDA-GDB | Version parsing and validation for GDB-based adapters |
| lldb.rs | LLDB native DAP adapter installer | Setting up C/C++/Rust/Swift debugging |
| mod.rs | Module exports for all adapters | Internal module organization |

## CUDA-GDB Architecture

CUDA-GDB supports two modes, automatically detected at setup time:

| Mode | When Used | Command |
|------|-----------|---------|
| Native DAP | cuda-gdb with GDB 14.1+ and DAP Python bindings (NVIDIA official installs) | `cuda-gdb -i=dap` |
| cdt-gdb-adapter bridge | cuda-gdb without native DAP (e.g., Arch Linux minimal build) | `cdtDebugAdapter --config={"gdb":"/path/to/cuda-gdb"}` |

### Stop-on-Entry Behavior

Different adapters use different parameters for stop-on-entry:

| Adapter | Parameter | Notes |
|---------|-----------|-------|
| lldb-dap | `stopOnEntry: true` | Standard DAP |
| GDB native DAP | `stopAtBeginningOfMainSubprogram: true` | GDB-specific |
| Delve (Go) | `stopAtEntry: true` | Delve-specific |
| cdt-gdb-adapter | Not supported | Use `--break main` instead |

### Initial Breakpoints

For adapters that don't support stop-on-entry, use the `--break` flag:

```bash
debugger start ./program --break main
debugger start ./program --break vectorAdd --break main.cu:42
```

Initial breakpoints are set during the DAP configuration phase (between `initialized` event and `configurationDone`), ensuring they're active before program execution begins.

## Key Functions

### `gdb_common.rs`

- `parse_gdb_version(output)`: Extracts GDB version from `--version` output. Handles cuda-gdb's "exec:" wrapper line.
- `is_gdb_version_sufficient(version)`: Checks if version ≥14.1 for DAP support.
- `get_gdb_version(path)`: Async helper that runs `--version` and parses output.

### `cuda_gdb.rs`

- `has_native_dap_support(path)`: Tests if cuda-gdb supports `-i=dap` by checking for "Interpreter `dap' unrecognized" error.
- `find_cuda_gdb()`: Searches versioned CUDA installs, `/usr/local/cuda`, `/opt/cuda`, `CUDA_HOME`, then PATH.
- `find_cdt_gdb_adapter()`: Searches PATH, nvm installs, npm global directories.
