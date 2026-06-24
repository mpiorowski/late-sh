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
# Stage 0a: NetHack - Build the door game binary from verified upstream source
# ==============================================================================
# We compile the official NetHack release from source rather than installing the
# distro "nethack-console" package, because the Debian package lags well behind
# upstream (bookworm ships 3.6.6; we want 5.0.0). The source tarball's SHA-256 is
# verified against the checksum published on nethack.org BEFORE the build runs;
# `sha256sum -c` fails the build closed on any mismatch.
#
# URL + checksum are VERIFIED against https://www.nethack.org/v500/download-src.html
# (tarball downloaded and hashed 2026-06-24). Build recipe follows the release's
# own sys/unix/NewInstall.unx, and the PREFIX/HACKDIR overrides were confirmed to
# resolve correctly via `make -pn`.
FROM debian:${DEBIAN_VERSION}-slim AS nethack-build

ARG NETHACK_VERSION=5.0.0
ARG NETHACK_TARBALL=nethack-500-src.tgz
ARG NETHACK_URL=https://www.nethack.org/download/5.0.0/nethack-500-src.tgz
ARG NETHACK_SHA256=2959b7886aac76185b90aea0c9f80d14343f604de0ae96b3dd2a760f7ab3bde9
# PREFIX holds the install tree; HACKDIR is the playground: data files plus
# saves/bones/dumplogs at run time, AND the dir compiled into the binary
# (-DHACKDIR). We deliberately do NOT set NETHACKDIR in the app, so this
# compile-time path MUST equal the runtime playground path.
ARG NETHACK_PREFIX=/opt/nethack
ARG NETHACK_HACKDIR=/var/games/nethack

# build-essential + flex/bison + ncurses headers cover the tty/curses build;
# groff-base lets the install build its man pages.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    build-essential \
    flex \
    bison \
    libncursesw5-dev \
    groff-base \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
RUN curl -fsSL -o "${NETHACK_TARBALL}" "${NETHACK_URL}" \
    && echo "${NETHACK_SHA256}  ${NETHACK_TARBALL}" | sha256sum -c - \
    && tar -xzf "${NETHACK_TARBALL}" \
    && rm "${NETHACK_TARBALL}"

# Canonical 5.0.0 unix build (see sys/unix/NewInstall.unx): configure from the
# linux.500 hints (run from sys/unix), fetch+verify Lua, then build and install.
# `make fetch-Lua` downloads Lua over the network but verifies it against the
# pinned checksums in submodules/CHKSUMS (shipped inside this already-verified
# tarball), so it is integrity-checked though not offline. PREFIX/HACKDIR are
# passed as make overrides (the documented config mechanism); the binary + data
# install into HACKDIR with -DHACKDIR baked to the same path. The final asserts
# the binary landed where the runtime stages expect it.
WORKDIR /build/NetHack-${NETHACK_VERSION}
RUN cd sys/unix && sh setup.sh hints/linux.500 && cd ../.. \
    && make fetch-Lua \
    && make PREFIX=${NETHACK_PREFIX} HACKDIR=${NETHACK_HACKDIR} GAMEUID=root GAMEGRP=games all \
    && make PREFIX=${NETHACK_PREFIX} HACKDIR=${NETHACK_HACKDIR} GAMEUID=root GAMEGRP=games install \
    && test -x ${NETHACK_HACKDIR}/nethack

# ==============================================================================
# Stage 0: Base - Common system dependencies
# ==============================================================================
FROM rust:${RUST_VERSION}-slim-${DEBIAN_VERSION} AS base

# Install system dependencies. libncursesw6 is the runtime lib for the NetHack
# door binary, which we build from source in the nethack-build stage and copy in
# below (the distro nethack-console package lags upstream, so we don't use it).
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
    libncursesw6 \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/lib/late-nethack && chmod 0777 /var/lib/late-nethack

# NetHack door game: the from-source binary lives inside its playground
# (/var/games/nethack/nethack) and self-locates via its compiled-in HACKDIR, so
# we copy just that tree and symlink the binary to /usr/games/nethack (the
# LATE_NETHACK_BIN default). World-writable so the runtime user can save bones.
COPY --from=nethack-build /var/games/nethack /var/games/nethack
RUN mkdir -p /usr/games \
    && ln -sf /var/games/nethack/nethack /usr/games/nethack \
    && chmod -R 0777 /var/games/nethack

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
COPY vendor vendor

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
COPY vendor vendor
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo chef cook --release --features otel --recipe-path recipe.json -p late-core -p late-ssh -p late-web

# Copy actual source code
COPY Cargo.toml Cargo.lock ./
COPY late-core late-core
COPY late-ssh late-ssh
COPY late-web late-web
COPY vendor vendor
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
    libncursesw6 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --user-group late \
    && mkdir -p /var/lib/late-nethack && chmod 0777 /var/lib/late-nethack

# NetHack door game: from-source binary (inside its playground, self-locating via
# compiled-in HACKDIR) plus data files and a writable saves/bones playground (see
# the nethack-build stage). LATE_NETHACK_BIN defaults to /usr/games/nethack. The
# playground is world-writable so the late user can save bones; for production,
# back it with persistent storage mounted at /var/games/nethack.
COPY --from=nethack-build /var/games/nethack /var/games/nethack
RUN mkdir -p /usr/games \
    && ln -sf /var/games/nethack/nethack /usr/games/nethack \
    && chmod -R 0777 /var/games/nethack

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
