# syntax=docker/dockerfile:1.20
#
# Multi-stage Dockerfile for parrot-agent.
# Builds a release binary, copies the pre-built Web UI, and runs as a
# non-root user with systemd-compatible Type=notify semantics.
#
# Build:
#   docker build --progress=plain -t parrot-agent:latest .
#
# Run (standalone, for dev only — use systemd in production):
#   docker run --rm -p 3100:3100 \
#     -e DATABASE_URL=postgres://user:pass@host:5432/db \
#     -e DEPLOYMENT_MODE=authenticated \
#     -e PARROT_UI_DIR=/app/ui \
#     parrot-agent:latest

# ── Stage 1: Rust build ──────────────────────────────────────────────────────
FROM rust:1.85-slim AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libpq-dev git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY parrot-agent/Cargo.toml parrot-agent/Cargo.lock ./
COPY parrot-agent/crates ./crates
COPY parrot-agent/migrations ./migrations
COPY parrot-agent/build.rs ./ 2>/dev/null || true

ARG CARGO_PROFILE=release
ARG PARROT_VERSION=0.0.0-local
ARG PARROT_BUILD_COMMIT=unknown
ARG PARROT_BUILD_TIME=unknown

ENV PARROT_VERSION=${PARROT_VERSION} \
    PARROT_BUILD_COMMIT=${PARROT_BUILD_COMMIT} \
    PARROT_BUILD_TIME=${PARROT_BUILD_TIME}

RUN cargo build --profile ${CARGO_PROFILE} -p parrot-server \
    && mkdir -p /release/bin \
    && cp target/${CARGO_PROFILE}/parrot-server /release/bin/ \
    && chmod 755 /release/bin/parrot-server

# ── Stage 2: Runtime ─────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
       ca-certificates curl jq postgresql-common \
    && rm -rf /var/lib/apt/lists/*

# Non-root user matching the systemd unit expectations
RUN groupadd -g 1000 parrot \
    && useradd -u 1000 -g parrot -s /usr/sbin/nologin \
       -d /var/lib/parrot -m parrot

# Copy binary
COPY --from=builder /release/bin/parrot-server /usr/local/bin/parrot-server
RUN chmod 755 /usr/local/bin/parrot-server

# Copy pre-built Web UI (built outside the image; must be present at runtime)
# Set PARROT_UI_DIR=/app/ui when starting the container.
COPY --chown=parrot:parrot parrot-web-ui/dist /app/ui

# Directories expected by the systemd unit
RUN mkdir -p /var/lib/parrot /var/log/parrot \
    && chown -R parrot:parrot /var/lib/parrot /var/log/parrot /app/ui \
    && chmod 750 /var/lib/parrot /var/log/parrot

USER parrot
WORKDIR /app

ENV PATH="/usr/local/bin:${PATH}" \
    DEPLOYMENT_MODE=authenticated \
    HOST=0.0.0.0 \
    PORT=3100

EXPOSE 3100

ENTRYPOINT ["parrot-server"]
CMD ["--config", "/etc/parrot/parrot.conf"]
