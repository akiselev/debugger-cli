# GDB debug adapter for C/C++ debugging
# Tests: GDB with native DAP support (GDB >= 14.1)

FROM ghcr.io/akiselev/debugger-cli:base AS base

USER root

# Install GDB with DAP support
# Note: Debian bookworm has GDB 13.x, we need 14.1+ for native DAP
# For older GDB, cdt-gdb-adapter bridge is used
RUN apt-get update && apt-get install -y --no-install-recommends \
    gdb \
    gcc \
    g++ \
    && rm -rf /var/lib/apt/lists/*

# Check GDB version
RUN gdb --version | head -1

# Install cdt-gdb-adapter as fallback for older GDB
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    npm install -g cdt-gdb-adapter && \
    rm -rf /var/lib/apt/lists/*

USER debugger

# Set environment for tests
ENV DEBUGGER_ADAPTER=gdb
ENV TEST_LANGUAGES="c,cpp"

# Default command compiles and runs tests
CMD ["bash", "-c", "gcc -g tests/fixtures/simple.c -o tests/fixtures/test_simple_c && debugger test tests/scenarios/hello_world_c.yml --verbose"]
