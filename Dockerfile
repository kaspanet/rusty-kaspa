# Multi-stage build for Rusty Kaspa
FROM rust:1.82-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    libclang-dev \
    clang \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# Set working directory
WORKDIR /app

# Copy the entire workspace
COPY . .

# Build the project in release mode
RUN cargo build --release --bin kaspad --bin kaspa-cli --bin kaspa-wrpc-proxy

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -r -s /bin/false kaspa

# Create necessary directories
RUN mkdir -p /app/data /app/logs /app/config \
    && chown -R kaspa:kaspa /app

# Copy binaries from builder stage
COPY --from=builder /app/target/release/kaspad /usr/local/bin/
COPY --from=builder /app/target/release/kaspa-cli /usr/local/bin/
COPY --from=builder /app/target/release/kaspa-wrpc-proxy /usr/local/bin/

# Set ownership
RUN chown kaspa:kaspa /usr/local/bin/kaspad /usr/local/bin/kaspa-cli /usr/local/bin/kaspa-wrpc-proxy

# Switch to non-root user
USER kaspa

# Set working directory
WORKDIR /app

# Expose default ports
# Mainnet: gRPC 16110, wRPC Borsh 17110, wRPC JSON 18110, P2P 16111
# Testnet: gRPC 16210, wRPC Borsh 17210, wRPC JSON 18210, P2P 16211
EXPOSE 16110 17110 18110 16111 16210 17210 18210 16211

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
    CMD kaspa-cli --server localhost:16110 ping || exit 1

# Default command
CMD ["kaspad", "--utxoindex"]
