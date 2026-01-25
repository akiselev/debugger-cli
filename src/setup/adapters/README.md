# Debug Adapter Installers

This directory contains installer implementations for various debug adapters. Each adapter implements the `Installer` trait and handles detection, installation, and verification of debugger binaries.

## GDB and CUDA-GDB

### Native DAP vs MI Adapter

GDB ≥14.1 includes native DAP support via the `-i=dap` interpreter flag. This implementation uses native DAP rather than the MI (Machine Interface) adapter approach for three reasons:

1. **Zero dependencies**: Native DAP requires only the GDB binary, while cdt-gdb-adapter requires Node.js runtime (50MB+ dependency)
2. **Simpler integration**: Native DAP uses stdin/stdout transport identical to lldb-dap, reusing existing `DapClient::spawn()` patterns
3. **Future-proof**: NVIDIA CUDA Toolkit ships CUDA-GDB based on GDB 14.2, inheriting native DAP support from upstream

The `-i=dap` flag must be passed at startup; GDB cannot switch interpreters mid-session.

### Version Requirements

GDB native DAP requires Python support, added in GDB 14.1. Installers verify version at setup time and return `Broken` status for older versions with upgrade instructions.

CUDA-GDB 13.x is based on GDB 14.2 and inherits DAP support. The installer validates DAP availability during verification via `verify_dap_adapter()`.

### CUDA Toolkit Path Detection

CUDA-GDB installer searches three locations in priority order:

1. `/usr/local/cuda/bin/cuda-gdb` - NVIDIA's standard installation path (checked first to catch default installations)
2. `$CUDA_HOME/bin/cuda-gdb` - Custom toolkit installations via environment variable
3. `cuda-gdb` in PATH - Fallback for wrapper scripts and non-standard setups

This order prioritizes official NVIDIA installations over custom configurations.

### Separate Adapters for GDB vs CUDA-GDB

Despite sharing 90% of implementation patterns, GDB and CUDA-GDB use separate adapters because they differ in:

- **Platform support**: GDB works on Linux/macOS/Windows, CUDA-GDB GPU debugging requires Linux (NVIDIA driver limitation)
- **Path detection**: GDB found in PATH, CUDA-GDB in CUDA Toolkit locations
- **Language mapping**: GDB for C/C++, CUDA-GDB for CUDA (may overlap with C/C++)
- **Version requirements**: GDB ≥14.1, CUDA-GDB tied to CUDA Toolkit version

Shared logic (version parsing, validation) lives in `gdb_common.rs`.
