# Multi-stage build for portal-app and portal-cli
#
# Workspace declares `rust-version = "1.85"` (Cargo.toml) for Edition 2024, but
# transitive deps have bumped their MSRV — aws-sdk-s3/sso/ssooidc/sts and home
# all require rustc ≥ 1.88 as of this Cargo.lock. Bump the base image here when
# a `cargo update` pulls in deps that need a newer compiler.

FROM rust:1.88-slim-bookworm AS builder

# Install build dependencies
# `curl` is required by utoipa-swagger-ui's build.rs to fetch the Swagger UI
# bundle at compile time.
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
COPY .sqlx ./.sqlx

# Build release binaries
ENV SQLX_OFFLINE=true
RUN cargo build --release -p portal-app -p portal-cli -p portal-scanner

# Runtime image
FROM debian:bookworm-slim

# Install runtime dependencies.
# `curl` is only here for the HEALTHCHECK below (the image has no other
# HTTP client); `postgresql-client` is used by entrypoint.sh to wait for the DB.
RUN apt-get update && apt-get install -y \
    libpq5 \
    ca-certificates \
    postgresql-client \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries from builder.
# Note: the portal-app *package* produces a binary named `portal`
# (see crates/portal-app/Cargo.toml `[[bin]] name = "portal"`).
COPY --from=builder /app/target/release/portal /usr/local/bin/
COPY --from=builder /app/target/release/portal-cli /usr/local/bin/
COPY --from=builder /app/target/release/portal-scanner /usr/local/bin/

# Copy migrations for runtime
COPY migrations /app/migrations

# Copy and set up entrypoint script
COPY scripts/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Set working directory for migrations path
WORKDIR /app

# Run as an unprivileged system user. The only path the app writes to is the
# local evidence/upload store, which defaults to `./uploads` relative to this
# WORKDIR (AppState storage_config.base_path); create it up front and hand
# both it and /app to the portal user. Everything else (binaries, migrations,
# entrypoint) stays root-owned and read-only to the runtime user.
RUN useradd -r -u 10001 -d /app -s /usr/sbin/nologin portal \
    && mkdir -p /app/uploads \
    && chown -R portal:portal /app
USER portal

EXPOSE 3000

# Liveness probe against the existing /health endpoint (which also pings the
# DB pool, so a container whose database vanished reports unhealthy).
# start-period covers migrations running at boot.
HEALTHCHECK --interval=30s --timeout=5s --start-period=60s --retries=3 \
    CMD curl -fsS "http://127.0.0.1:${PORT:-3000}/health" || exit 1

ENTRYPOINT ["/entrypoint.sh"]
CMD ["portal"]
