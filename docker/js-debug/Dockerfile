# js-debug adapter for JavaScript/TypeScript debugging
# Tests: vscode-js-debug adapter with Node.js runtime

FROM ghcr.io/akiselev/debugger-cli:base AS base

USER root

# Install Node.js (LTS)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# Verify Node.js
RUN node --version && npm --version

# Install js-debug globally (the VS Code JavaScript Debugger)
RUN npm install -g @anthropic-ai/vscode-js-debug 2>/dev/null || \
    npm install -g @anthropic-ai/js-debug 2>/dev/null || \
    echo "Will install js-debug via debugger setup"

# Install TypeScript for test fixtures
RUN npm install -g typescript

USER debugger

# Ensure npm global bin is in PATH
ENV PATH="/home/debugger/.npm-global/bin:${PATH}"
ENV NPM_CONFIG_PREFIX="/home/debugger/.npm-global"

# Set environment for tests
ENV DEBUGGER_ADAPTER=js-debug
ENV TEST_LANGUAGES="javascript,typescript"

# Build TypeScript fixtures if needed
RUN cd /home/debugger/debugger-cli/tests/fixtures && \
    npm install 2>/dev/null || true && \
    npx tsc 2>/dev/null || true

# Default command runs tests
CMD ["bash", "-c", "debugger setup js-debug && debugger test tests/scenarios/hello_world_js.yml --verbose && debugger test tests/scenarios/hello_world_ts.yml --verbose"]
