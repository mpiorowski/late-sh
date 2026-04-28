# syntax=docker/dockerfile:1.4
#
# Multi-stage Dockerfile for late.sh services using cargo-chef
# Optimized for fast rebuilds via Docker layer caching
#
# Build SSH:  docker build --target runtime-ssh -t late-ssh .
# Build Web:  docker build --target runtime-web -t late-web .
# Run:        docker run -p 2222:2222 late-ssh

ARG RUST_VERSION=1.92
ARG DEBIAN_VERSION=bookworm

# ==============================================================================
# Stage 0: Base - Common system dependencies
# ==============================================================================
FROM rust:${RUST_VERSION}-slim-${DEBIAN_VERSION} AS base

# Install system dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake \
    make \
    pkg-config \
    libssl-dev \
    perl \
    clang \
    mold \
    nodejs \
    npm \
    && rm -rf /var/lib/apt/lists/*

# Configure cargo to use mold linker
RUN echo '[target.x86_64-unknown-linux-gnu]\nlinker = "clang"\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n\n[target.aarch64-unknown-linux-gnu]\nlinker = "clang"\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]' >> /usr/local/cargo/config.toml

WORKDIR /app

# ==============================================================================
# Stage 1: Chef - Install cargo-chef
# ==============================================================================
FROM base AS chef

RUN cargo install cargo-chef --locked

# ==============================================================================
# Stage 2: Planner - Generate recipe.json (dependency manifest)
# ==============================================================================
FROM chef AS planner

# Copy workspace manifests
COPY Cargo.toml Cargo.lock ./
COPY late-core/Cargo.toml late-core/Cargo.toml
COPY late-ssh/Cargo.toml late-ssh/Cargo.toml
COPY late-web/Cargo.toml late-web/Cargo.toml
COPY late-cli/Cargo.toml late-cli/Cargo.toml

# Create dummy source files for cargo-chef to analyze
RUN mkdir -p late-core/src late-ssh/src late-web/src late-cli/src && \
    echo "fn main() {}" > late-core/src/lib.rs && \
    echo "fn main() {}" > late-ssh/src/main.rs && \
    echo "fn main() {}" > late-web/src/main.rs && \
    echo "fn main() {}" > late-cli/src/main.rs

RUN cargo chef prepare --recipe-path recipe.json

# ==============================================================================
# Stage 3: Builder - Build dependencies (cached), then all binaries
# ==============================================================================
FROM chef AS builder

# Copy recipe and cook ALL dependencies (cached until any dep changes)
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo chef cook --release --features otel --recipe-path recipe.json -p late-core -p late-ssh -p late-web

# Copy actual source code
COPY Cargo.toml Cargo.lock ./
COPY late-core late-core
COPY late-ssh late-ssh
COPY late-web late-web
COPY late-cli/Cargo.toml late-cli/Cargo.toml
RUN mkdir -p late-cli/src && echo "fn main() {}" > late-cli/src/main.rs
# Build deployable binaries only (late-cli excluded - local CLI tooling)
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --release --features otel -p late-ssh -p late-web && \
    cp /app/target/release/late-ssh /app/late-ssh-bin && \
    cp /app/target/release/late-web /app/late-web-bin

# Build frontend assets
RUN cd late-web && npm install && npm run tailwind:build

# ==============================================================================
# Stage 3b: Dev base - Rust toolchain + dev deps
# ==============================================================================
FROM base AS dev-base

RUN cargo install cargo-watch --locked

ENV CARGO_TARGET_DIR=/app/target

# ==============================================================================
# Stage 3c: Dev targets
# ==============================================================================
FROM dev-base AS dev-ssh
CMD ["cargo", "watch", "-w", "late-ssh", "-x", "run --features otel -p late-ssh"]

FROM dev-base AS dev-web
CMD ["bash", "-c", "cd /app/late-web && npm install && npm run tailwind:build && (npm run tailwind:watch &) && cd /app && cargo watch -w late-web -x 'run --features otel -p late-web'"]

# ==============================================================================
# Stage 4a: Runtime base - Common runtime setup
# ==============================================================================
FROM debian:${DEBIAN_VERSION}-slim AS runtime-base

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --user-group late

WORKDIR /app
USER late
ENV RUST_LOG=info

# ==============================================================================
# Stage 4b: Runtime SSH - SSH server
# ==============================================================================
FROM runtime-base AS runtime-ssh

COPY --from=builder /app/late-ssh-bin /app/late-ssh

EXPOSE 2222

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD timeout 2 bash -c 'exec 3<>/dev/tcp/localhost/4000; printf "GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n" >&3; head -n 1 <&3 | grep -q "200"' || exit 1

CMD ["/app/late-ssh"]

# ==============================================================================
# Stage 4c: Runtime Web - HTTP server
# ==============================================================================
FROM runtime-base AS runtime-web

COPY --from=builder /app/late-web-bin /app/late-web-bin
COPY --from=builder /app/late-web/static /app/late-web/static

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD timeout 2 bash -c '</dev/tcp/localhost/8080' || exit 1

CMD ["/app/late-web-bin"]
