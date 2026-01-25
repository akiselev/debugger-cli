# Base image for debugger-cli e2e testing
# Contains Rust toolchain and common build dependencies

FROM rust:1.83-bookworm

# Prevent interactive prompts
ENV DEBIAN_FRONTEND=noninteractive

# Install common build dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    cmake \
    curl \
    git \
    ca-certificates \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for testing
RUN useradd -m -s /bin/bash debugger
WORKDIR /home/debugger/debugger-cli

# Pre-cache cargo registry
RUN mkdir -p /home/debugger/.cargo && \
    chown -R debugger:debugger /home/debugger

# Copy project files
COPY --chown=debugger:debugger . .

# Build the debugger CLI
RUN cargo build --release && \
    cp target/release/debugger /usr/local/bin/

# Switch to non-root user
USER debugger

ENTRYPOINT ["/bin/bash"]
