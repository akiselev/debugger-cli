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
