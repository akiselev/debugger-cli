# Docker E2E Test Images

This directory contains Dockerfiles for running comprehensive end-to-end tests across all supported debug adapters.

## Images

| Image | Debug Adapter | Languages | Base |
|-------|--------------|-----------|------|
| `base` | - | Rust (build only) | rust:1.83-bookworm |
| `lldb` | lldb-dap | C, C++, Rust, Swift | base |
| `delve` | dlv | Go | base |
| `debugpy` | debugpy | Python | base |
| `js-debug` | vscode-js-debug | JavaScript, TypeScript | base |
| `gdb` | gdb (native DAP) or cdt-gdb-adapter | C, C++ | base |
| `cuda-gdb` | cuda-gdb | CUDA C/C++ | base (requires nvidia-docker) |

## Usage

### Using Docker Compose

```bash
# Build all images
docker compose build

# Run specific adapter tests
docker compose up lldb
docker compose up delve
docker compose up debugpy
docker compose up js-debug
docker compose up gdb

# Run all tests
docker compose up
```

### Using the test script

```bash
# Run all tests
./scripts/run-e2e-tests.sh

# Run specific adapter tests
./scripts/run-e2e-tests.sh lldb
./scripts/run-e2e-tests.sh js-debug
```

### Building individual images

```bash
# Build base image first
docker build -t debugger-cli:base -f docker/base/Dockerfile .

# Build adapter-specific image
docker build -t debugger-cli:lldb -f docker/lldb/Dockerfile .

# Run tests
docker run --rm debugger-cli:lldb
```

## Test Coverage

Each image runs the following test types:

1. **Scenario tests** (`debugger test tests/scenarios/*.yml`)
   - Hello world programs for each language
   - Breakpoints, stepping, variable inspection
   - Stack traces, expression evaluation

2. **Integration tests** (`cargo test --test integration`)
   - Rust test framework tests
   - More detailed feature coverage

## Adding New Adapters

1. Create a new Dockerfile in `docker/<adapter>/Dockerfile`
2. Base it on `ghcr.io/akiselev/debugger-cli:base`
3. Install the debug adapter and language toolchain
4. Add test scenarios in `tests/scenarios/`
5. Add to `docker-compose.yml`
6. Add to CI workflow in `.github/workflows/e2e-tests.yml`

## CUDA-GDB Notes

The CUDA-GDB image requires NVIDIA Container Runtime:

```bash
# Requires nvidia-docker2 installed
docker run --gpus all --rm debugger-cli:cuda-gdb
```

CUDA-GDB supports two modes automatically detected at setup:
- **Native DAP**: cuda-gdb with GDB 14.1+ and DAP Python bindings
- **cdt-gdb-adapter bridge**: Older cuda-gdb without native DAP
