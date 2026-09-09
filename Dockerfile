# Multi-stage build for minimal image size

# Build stage
FROM rust:1.83-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy Cargo files
COPY Cargo.toml ./
COPY Cargo.lock ./

# Create dummy main.rs to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy source code
COPY src ./src
COPY frontend ./frontend

# Build application
RUN touch src/main.rs && \
    cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/zggsds .

# Create directory for database and logs
RUN mkdir -p /app/data && \
    chown -R nobody:nogroup /app

# Switch to non-root user
USER nobody:nogroup

# Expose port
EXPOSE 3000

# Set environment variables
ENV RUST_LOG=info

# Run application
ENTRYPOINT ["./zggsds"]

