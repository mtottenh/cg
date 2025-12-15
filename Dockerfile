# Multi-stage build for portal-app and portal-cli
# Rust 1.85+ required for Edition 2024

FROM rust:1.85-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
COPY .sqlx ./.sqlx

# Build release binaries
ENV SQLX_OFFLINE=true
RUN cargo build --release -p portal-app -p portal-cli

# Runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libpq5 \
    ca-certificates \
    postgresql-client \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries from builder
COPY --from=builder /app/target/release/portal-app /usr/local/bin/
COPY --from=builder /app/target/release/portal-cli /usr/local/bin/

# Copy migrations for runtime
COPY migrations /app/migrations

# Copy and set up entrypoint script
COPY scripts/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Set working directory for migrations path
WORKDIR /app

EXPOSE 3000

ENTRYPOINT ["/entrypoint.sh"]
CMD ["portal-app"]
