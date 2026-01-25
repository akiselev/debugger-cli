# Delve debug adapter for Go debugging
# Tests: dlv DAP adapter with TCP transport

FROM ghcr.io/akiselev/debugger-cli:base AS base

USER root

# Install Go
ENV GOLANG_VERSION=1.22.0
RUN curl -LO https://go.dev/dl/go${GOLANG_VERSION}.linux-amd64.tar.gz && \
    tar -C /usr/local -xzf go${GOLANG_VERSION}.linux-amd64.tar.gz && \
    rm go${GOLANG_VERSION}.linux-amd64.tar.gz

ENV PATH="/usr/local/go/bin:/home/debugger/go/bin:${PATH}"
ENV GOPATH="/home/debugger/go"

# Switch to debugger user for go install
USER debugger

# Install Delve
RUN go install github.com/go-delve/delve/cmd/dlv@latest

# Verify Delve is available
RUN dlv version

# Set environment for tests
ENV DEBUGGER_ADAPTER=go
ENV TEST_LANGUAGES="go"

# Default command runs tests
CMD ["bash", "-c", "go build -gcflags='all=-N -l' -o tests/e2e/test_go tests/e2e/hello_world.go && debugger test tests/scenarios/hello_world_go.yml --verbose"]
