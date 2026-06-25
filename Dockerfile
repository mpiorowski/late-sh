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
# PREFIX holds the install tree; HACKDIR is the read-only playground: data files
# AND the dir compiled into the binary (-DHACKDIR). We deliberately do NOT set
# NETHACKDIR in the app, so this compile-time path MUST equal the runtime path.
ARG NETHACK_PREFIX=/opt/nethack
ARG NETHACK_HACKDIR=/var/games/nethack
# VAR_PLAYGROUND splits the WRITABLE state (save/, bones, locks, record, level,
# trouble) out of HACKDIR so the latter can stay a read-only image layer while
# this dir is backed by a persistent volume. NetHack's own supported knob for
# "static playground on a read-only filesystem" (include/unixconf.h). At runtime
# unixmain.c::chdirx() points the writable prefixes here and still chdir()s to
# HACKDIR, so read-only data files keep loading from the image. Must equal the
# VARDIR install path and the PVC mount path in infra/nethack.tf.
ARG NETHACK_VAR_PLAYGROUND=/var/games/nethack-var

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
# install into HACKDIR with -DHACKDIR baked to the same path.
#
# VAR_PLAYGROUND is NOT reachable via the PREFIX/HACKDIR make overrides, so we
# define it directly in include/unixconf.h (the documented edit point) before
# building, and pass VARDIR=$NETHACK_VAR_PLAYGROUND so `make install` creates and
# seeds that dir (save/ + record/logfile/perm/...). The grep fails the build
# closed if upstream ever moves the commented VAR_PLAYGROUND line, since a silent
# sed miss would leave saves writing into HACKDIR. The asserts confirm both the
# binary (HACKDIR) and the writable seed (save/ under VAR_PLAYGROUND) landed.
#
# We also DISABLE NetHack's in-game shell ('!') and suspend ('^Z') escapes at
# compile time by removing their `#define`s in unixconf.h. late-ssh accepts
# anonymous SSH and runs the game as the service user inside the app container; a
# shell escape would hand an attacker a shell as that user (able to read the
# parent's /proc environ, reach in-cluster services, etc.), which env-clearing the
# child alone can't fully prevent. Removing the defines compiles the escape code
# out entirely, so no sysconf edit or missing file can re-enable it. The `!` grep
# fails the build closed if the defines aren't gone.
WORKDIR /build/NetHack-${NETHACK_VERSION}
RUN sed -i "s|^/\* #define VAR_PLAYGROUND .*|#define VAR_PLAYGROUND \"${NETHACK_VAR_PLAYGROUND}\"|" include/unixconf.h \
    && grep -qx "#define VAR_PLAYGROUND \"${NETHACK_VAR_PLAYGROUND}\"" include/unixconf.h \
    && sed -i 's|^#define SHELL\b.*|/* SHELL disabled by late.sh: no in-game shell escape */|;s|^#define SUSPEND\b.*|/* SUSPEND disabled by late.sh */|' include/unixconf.h \
    && ! grep -qE '^#define (SHELL|SUSPEND)\b' include/unixconf.h \
    && cd sys/unix && sh setup.sh hints/linux.500 && cd ../.. \
    && make fetch-Lua \
    && make PREFIX=${NETHACK_PREFIX} HACKDIR=${NETHACK_HACKDIR} VARDIR=${NETHACK_VAR_PLAYGROUND} GAMEUID=root GAMEGRP=games all \
    && make PREFIX=${NETHACK_PREFIX} HACKDIR=${NETHACK_HACKDIR} VARDIR=${NETHACK_VAR_PLAYGROUND} GAMEUID=root GAMEGRP=games install \
    && test -x ${NETHACK_HACKDIR}/nethack \
    && test -d ${NETHACK_VAR_PLAYGROUND}/save

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

# NetHack door game: the from-source binary lives inside its read-only playground
# (/var/games/nethack/nethack) and self-locates via its compiled-in HACKDIR; the
# writable state (saves/bones/locks/record) lives in /var/games/nethack-var via
# the baked-in VAR_PLAYGROUND. We copy both trees and symlink the binary to
# /usr/games/nethack (the LATE_NETHACK_BIN default). Dev runs as root, so the
# writable dir is world-writable; prod chowns it on the PVC (infra/nethack.tf).
COPY --from=nethack-build /var/games/nethack /var/games/nethack
COPY --from=nethack-build /var/games/nethack-var /var/games/nethack-var
RUN mkdir -p /usr/games \
    && ln -sf /var/games/nethack/nethack /usr/games/nethack \
    && chmod -R 0777 /var/games/nethack-var

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
COPY late-nethack/Cargo.toml late-nethack/Cargo.toml
COPY vendor vendor

# Create dummy source files for cargo-chef to analyze
RUN mkdir -p late-core/src late-ssh/src late-web/src late-cli/src late-nethack/src && \
    echo "fn main() {}" > late-core/src/lib.rs && \
    echo "fn main() {}" > late-ssh/src/main.rs && \
    echo "fn main() {}" > late-web/src/main.rs && \
    echo "fn main() {}" > late-cli/src/main.rs && \
    echo "fn main() {}" > late-nethack/src/main.rs

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
    cargo chef cook --release --features otel --recipe-path recipe.json -p late-core -p late-ssh -p late-web -p late-nethack

# Copy actual source code
COPY Cargo.toml Cargo.lock ./
COPY late-core late-core
COPY late-ssh late-ssh
COPY late-web late-web
COPY late-nethack late-nethack
COPY vendor vendor
COPY late-cli/Cargo.toml late-cli/Cargo.toml
RUN mkdir -p late-cli/src && echo "fn main() {}" > late-cli/src/main.rs
# Build deployable binaries only (late-cli excluded - local CLI tooling).
# late-nethack has no otel feature; it is built without the workspace feature flag.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --release --features otel -p late-ssh -p late-web && \
    cargo build --release -p late-nethack && \
    cp /app/target/release/late-ssh /app/late-ssh-bin && \
    cp /app/target/release/late-web /app/late-web-bin && \
    cp /app/target/release/late-nethack /app/late-nethack-bin

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

# NetHack host: serves the game over SSH (see late-nethack). dev-base derives from
# `base`, which already has the from-source nethack binary + playground, so the
# default LATE_NETHACK_BIN (/usr/games/nethack) resolves here.
FROM dev-base AS dev-nethack
CMD ["cargo", "watch", "-w", "late-nethack", "-x", "run -p late-nethack"]

# ==============================================================================
# Stage 4a: Runtime base - Common runtime setup
# ==============================================================================
FROM debian:${DEBIAN_VERSION}-slim AS runtime-base

# Common runtime: late-ssh and late-web only. The NetHack binary, its ncurses
# runtime, and playground now live solely in runtime-nethack (the late-nethack host),
# so this base no longer ships them.
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

# ==============================================================================
# Stage 4d: Runtime NetHack - the late-nethack host (game served over SSH)
# ==============================================================================
# Owns everything the game needs: the from-source nethack binary + read-only data
# files in HACKDIR (/var/games/nethack, self-locating via compiled-in HACKDIR),
# the writable saves/bones playground in /var/games/nethack-var (baked-in
# VAR_PLAYGROUND; backed by a PVC in prod), the ncurses runtime, and the per-player
# .nethackrc HOME. LATE_NETHACK_BIN defaults to /usr/games/nethack.
FROM runtime-base AS runtime-nethack
USER root
RUN apt-get update && apt-get install -y --no-install-recommends \
    libncursesw6 \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/lib/late-nethack && chown late:late /var/lib/late-nethack
COPY --from=nethack-build /var/games/nethack /var/games/nethack
COPY --from=nethack-build /var/games/nethack-var /var/games/nethack-var
RUN mkdir -p /usr/games \
    && ln -sf /var/games/nethack/nethack /usr/games/nethack \
    && chown -R late:late /var/games/nethack-var
COPY --from=builder /app/late-nethack-bin /app/late-nethack
USER late

EXPOSE 2323

CMD ["/app/late-nethack"]
