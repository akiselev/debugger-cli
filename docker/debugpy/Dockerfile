# Debugpy debug adapter for Python debugging
# Tests: debugpy adapter with Python scripts

FROM ghcr.io/akiselev/debugger-cli:base AS base

USER root

# Install Python and pip
RUN apt-get update && apt-get install -y --no-install-recommends \
    python3 \
    python3-pip \
    python3-venv \
    && rm -rf /var/lib/apt/lists/*

# Install debugpy globally
RUN pip3 install --break-system-packages debugpy

# Verify debugpy is available
RUN python3 -c "import debugpy; print(f'debugpy {debugpy.__version__}')"

USER debugger

# Set environment for tests
ENV DEBUGGER_ADAPTER=debugpy
ENV TEST_LANGUAGES="python"

# Default command runs tests
CMD ["bash", "-c", "debugger test tests/scenarios/hello_world_python.yml --verbose"]
