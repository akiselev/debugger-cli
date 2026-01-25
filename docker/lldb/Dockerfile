# LLDB debug adapter for C/C++/Rust/Swift debugging
# Tests: lldb-dap adapter with native binaries

FROM ghcr.io/akiselev/debugger-cli:base AS base

USER root

# Install LLDB and development tools
RUN apt-get update && apt-get install -y --no-install-recommends \
    lldb \
    clang \
    libc++-dev \
    libc++abi-dev \
    && rm -rf /var/lib/apt/lists/*

# Verify lldb-dap is available
RUN which lldb-dap || which lldb-vscode || echo "LLDB DAP adapter not found"

USER debugger

# Set environment for tests
ENV DEBUGGER_ADAPTER=lldb-dap
ENV TEST_LANGUAGES="c,cpp,rust"

# Default command runs tests
CMD ["bash", "-c", "debugger test tests/scenarios/hello_world_c.yml --verbose && debugger test tests/scenarios/hello_world_rust.yml --verbose"]
