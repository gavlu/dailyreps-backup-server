# Build stage
FROM rust:1.83-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Copy source code
COPY . .

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/dailyreps-backup-server /usr/local/bin/

# Create data directory for redb database
RUN mkdir -p /data

# Set default environment variables
ENV DATABASE_PATH=/data/dailyreps.db
ENV SERVER_HOST=0.0.0.0
ENV SERVER_PORT=8080

# Expose port
EXPOSE 8080

# Run the server
CMD ["dailyreps-backup-server"]
