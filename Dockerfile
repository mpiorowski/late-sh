# syntax=docker/dockerfile:1.4
#
# Multi-stage Dockerfile for late.sh services using cargo-chef
# Optimized for fast rebuilds via Docker layer caching
#
# Build SSH:  docker build --target runtime-ssh -t late-ssh .
# Build Web:  docker build --target runtime-web -t late-web .
# Run:        docker run -p 2222:2222 late-ssh

ARG RUST_VERSION=1.97
ARG DEBIAN_VERSION=bookworm

# ==============================================================================
# Stage 0: Door game binaries - prebuilt images from docker/doors/
# ==============================================================================
# Each door game (the real upstream binary, compiled from verified source) has
# its own Dockerfile under docker/doors/ and its own workflow that builds and
# pushes the image (.github/workflows/<door>.yml). Pinning them here by tag
# means a door recipe rebuilds only when its own Dockerfile changes, never on
# ordinary image builds. Bump a tag when that door's recipe or upstream
# version changes.
FROM ghcr.io/mpiorowski/late-sh/door-nethack:5.0.0-r1 AS nethack-build
FROM ghcr.io/mpiorowski/late-sh/door-dopewars:1.6.2-r1 AS dopewars-build
FROM ghcr.io/mpiorowski/late-sh/door-dcss:0.34.1-r1 AS dcss-build
FROM ghcr.io/mpiorowski/late-sh/door-usurper:0.25-r1 AS usurper-build
FROM ghcr.io/mpiorowski/late-sh/door-brogue:1.15.1-r1 AS brogue-build

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
    libncurses6 \
    libglib2.0-0 \
    libcurl4 \
    liblua5.4-0 \
    libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/lib/late-nethack && chmod 0777 /var/lib/late-nethack \
    && mkdir -p /var/lib/late-dcss && chmod 0777 /var/lib/late-dcss \
    && mkdir -p /var/lib/late-brogue && chmod 0777 /var/lib/late-brogue \
    && mkdir -p /var/lib/late-usurper && chmod 0777 /var/lib/late-usurper

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

# dopewars door game: served over SSH by the late-dopewars host (see late-ssh
# dopewars proxy). The from-source terminal-only binary lives here so dev-dopewars
# (which derives from `base`) can run it; prod ships it in runtime-dopewars. Its
# runtime libs (glib2/ncursesw/curl) are installed above. LATE_DOPEWARS_BIN
# defaults to /usr/games/dopewars.
COPY --from=dopewars-build /dopewars /usr/games/dopewars

# DCSS door game: served over SSH by the late-dcss host (see late-ssh dcss
# proxy). The from-source console binary + data tree live here so dev-dcss
# (which derives from `base`) can run it; prod ships it in runtime-dcss. Its
# runtime libs (ncursesw/lua/sqlite) are installed above. LATE_DCSS_BIN
# defaults to /usr/games/crawl.
COPY --from=dcss-build /opt/dcss /opt/dcss
RUN ln -sf /opt/dcss/bin/crawl /usr/games/crawl

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
COPY late-dcss/Cargo.toml late-dcss/Cargo.toml
COPY late-brogue/Cargo.toml late-brogue/Cargo.toml
COPY late-dopewars/Cargo.toml late-dopewars/Cargo.toml
COPY late-usurper/Cargo.toml late-usurper/Cargo.toml
COPY late-webview/Cargo.toml late-webview/Cargo.toml
COPY vendor vendor

# Create dummy source files for cargo-chef to analyze. late-webview is never
# built in these images (CLI-only YouTube helper), but it is a workspace member
# and a late-cli path dependency, so its manifest and target stubs must exist
# for `cargo metadata` to resolve the workspace.
RUN mkdir -p late-core/src late-ssh/src late-web/src late-cli/src late-nethack/src late-dcss/src late-brogue/src late-dopewars/src late-usurper/src late-webview/src && \
    echo "fn main() {}" > late-core/src/lib.rs && \
    echo "fn main() {}" > late-ssh/src/main.rs && \
    echo "fn main() {}" > late-web/src/main.rs && \
    echo "fn main() {}" > late-cli/src/main.rs && \
    echo "fn main() {}" > late-nethack/src/main.rs && \
    echo "fn main() {}" > late-dcss/src/main.rs && \
    echo "fn main() {}" > late-brogue/src/main.rs && \
    echo "fn main() {}" > late-dopewars/src/main.rs && \
    echo "fn main() {}" > late-usurper/src/main.rs && \
    echo "" > late-webview/src/lib.rs && \
    echo "fn main() {}" > late-webview/src/main.rs

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
    cargo chef cook --release --features otel --recipe-path recipe.json -p late-core -p late-ssh -p late-web -p late-nethack -p late-dcss -p late-brogue -p late-dopewars -p late-usurper

# Copy actual source code
COPY Cargo.toml Cargo.lock ./
COPY late-core late-core
COPY late-ssh late-ssh
COPY late-web late-web
COPY late-nethack late-nethack
COPY late-dcss late-dcss
COPY late-brogue late-brogue
COPY late-dopewars late-dopewars
COPY late-usurper late-usurper
COPY vendor vendor
COPY late-cli/Cargo.toml late-cli/Cargo.toml
COPY late-webview/Cargo.toml late-webview/Cargo.toml
RUN mkdir -p late-cli/src late-webview/src && \
    echo "fn main() {}" > late-cli/src/main.rs && \
    echo "" > late-webview/src/lib.rs && \
    echo "fn main() {}" > late-webview/src/main.rs
# Build deployable binaries only (late-cli and late-webview excluded - local
# CLI tooling; the webview helper ships via deploy_cli.yml, not these images).
# late-nethack/late-dcss/late-dopewars have no otel feature; they are built
# without the workspace feature flag.
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --release --features otel -p late-ssh -p late-web && \
    cargo build --release -p late-nethack -p late-dcss -p late-brogue -p late-dopewars -p late-usurper && \
    cp /app/target/release/late-ssh /app/late-ssh-bin && \
    cp /app/target/release/late-web /app/late-web-bin && \
    cp /app/target/release/late-nethack /app/late-nethack-bin && \
    cp /app/target/release/late-dcss /app/late-dcss-bin && \
    cp /app/target/release/late-brogue /app/late-brogue-bin && \
    cp /app/target/release/late-dopewars /app/late-dopewars-bin && \
    cp /app/target/release/late-usurper /app/late-usurper-bin

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

# dopewars host: serves the game over SSH (see late-dopewars). dev-base derives
# from `base`, which already has the from-source dopewars binary + runtime libs,
# so the default LATE_DOPEWARS_BIN (/usr/games/dopewars) resolves here.
FROM dev-base AS dev-dopewars
CMD ["cargo", "watch", "-w", "late-dopewars", "-x", "run -p late-dopewars"]

# DCSS host: serves the game over SSH (see late-dcss). dev-base derives from
# `base`, which already has the from-source crawl binary + data tree, so the
# default LATE_DCSS_BIN (/usr/games/crawl) resolves here.
FROM dev-base AS dev-dcss
CMD ["cargo", "watch", "-w", "late-dcss", "-x", "run -p late-dcss"]

# Usurper host: serves the game over SSH (see late-usurper). This is the only dev
# target that needs the x86-64 upstream binaries + seed game tree; keeping the
# copy here prevents every other Compose service from building Usurper.
FROM dev-base AS dev-usurper
COPY --from=usurper-build /opt/usurper /opt/usurper
CMD ["cargo", "watch", "-w", "late-usurper", "-x", "run -p late-usurper"]

# Brogue host: serves the game over SSH (see late-brogue). This is the only dev
# target that needs the from-source curses binary; keeping the copy here (plus
# the /usr/games/brogue symlink for the default LATE_BROGUE_BIN) prevents every
# other Compose service from carrying it.
FROM dev-base AS dev-brogue
COPY --from=brogue-build /opt/brogue /opt/brogue
RUN mkdir -p /usr/games && ln -sf /opt/brogue/brogue /usr/games/brogue
CMD ["cargo", "watch", "-w", "late-brogue", "-x", "run -p late-brogue"]

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

# dopewars now runs in its own late-dopewars host (like nethack), reached over
# SSH, so its binary and curses runtime live solely in runtime-dopewars -- this
# image ships only the client.
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
# libncursesw6: nethack's curses runtime. ncurses-term: the EXTENDED terminfo DB
# (alacritty, rxvt, st, etc.) so clients on those terminals get native terminfo
# rather than the xterm-256color fallback. Terminals that ship their own terminfo
# (ghostty/kitty/wezterm) are still covered by the host's TERM fallback in
# late-nethack (effective_term), since they are not in ncurses-term.
RUN apt-get update && apt-get install -y --no-install-recommends \
    libncursesw6 \
    ncurses-term \
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

# ==============================================================================
# Stage 4e: Runtime dopewars - the late-dopewars host (game served over SSH)
# ==============================================================================
# Owns everything the game needs: the from-source terminal-only dopewars binary,
# its curses/glib runtime, and the writable directory holding the single shared
# high-score file (/var/lib/late-dopewars/dopewars.sco; backed by a PVC in prod
# so the leaderboard survives restarts). LATE_DOPEWARS_BIN defaults to
# /usr/games/dopewars, LATE_DOPEWARS_SCORE_FILE to that .sco path.
FROM runtime-base AS runtime-dopewars
USER root
# libglib2.0-0/libncursesw6/libcurl4: dopewars' runtime deps. ncurses-term: the
# EXTENDED terminfo DB (alacritty, rxvt, st, etc.) so clients on those terminals
# get native terminfo rather than the xterm-256color fallback. Terminals that
# ship their own terminfo (ghostty/kitty/wezterm) are covered by the host's TERM
# fallback in late-dopewars (effective_term), since they are not in ncurses-term.
RUN apt-get update && apt-get install -y --no-install-recommends \
    libglib2.0-0 \
    libncursesw6 \
    libcurl4 \
    ncurses-term \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/lib/late-dopewars && chown late:late /var/lib/late-dopewars
COPY --from=dopewars-build /dopewars /usr/games/dopewars
COPY --from=builder /app/late-dopewars-bin /app/late-dopewars
USER late

EXPOSE 2324

CMD ["/app/late-dopewars"]

# ==============================================================================
# Stage 4f: Runtime DCSS - the late-dcss host (game served over SSH)
# ==============================================================================
# Owns everything the game needs: the from-source console crawl binary + its
# read-only data tree (/opt/dcss, DATADIR baked in at build time), the curses/
# lua/sqlite runtime, and the writable playground HOME (/var/lib/late-dcss;
# backed by a PVC in prod so per-player saves under $HOME/.crawl survive
# restarts). LATE_DCSS_BIN defaults to /usr/games/crawl, LATE_DCSS_DATA_DIR to
# that playground path.
FROM runtime-base AS runtime-dcss
USER root
# libncursesw6/liblua5.4-0/libsqlite3-0: crawl's runtime deps. ncurses-term: the
# EXTENDED terminfo DB (alacritty, rxvt, st, etc.) so clients on those terminals
# get native terminfo rather than the xterm-256color fallback. Terminals that
# ship their own terminfo (ghostty/kitty/wezterm) are covered by the host's TERM
# fallback in late-dcss (effective_term), since they are not in ncurses-term.
RUN apt-get update && apt-get install -y --no-install-recommends \
    libncursesw6 \
    liblua5.4-0 \
    libsqlite3-0 \
    ncurses-term \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/lib/late-dcss && chown late:late /var/lib/late-dcss
COPY --from=dcss-build /opt/dcss /opt/dcss
RUN mkdir -p /usr/games && ln -sf /opt/dcss/bin/crawl /usr/games/crawl
COPY --from=builder /app/late-dcss-bin /app/late-dcss
USER late

EXPOSE 2325

CMD ["/app/late-dcss"]

# ==============================================================================
# Stage 4g: Runtime Usurper - the late-usurper host (game served over SSH)
# ==============================================================================
# Owns everything the game needs: the from-source statically-linked USURPER.EXE
# + EDITOR.EXE and the seed game tree in /opt/usurper (read-only image layer),
# plus the writable game dir /var/lib/late-usurper (backed by a PVC in prod so
# the shared world - players, gangs, king, news - survives restarts). The host
# copies missing seed files into the game dir at boot. No ncurses/terminfo: the
# game emits raw CP437 ANSI which the host transcodes to UTF-8 itself.
FROM runtime-base AS runtime-usurper
USER root
RUN mkdir -p /var/lib/late-usurper && chown late:late /var/lib/late-usurper
COPY --from=usurper-build /opt/usurper /opt/usurper
COPY --from=builder /app/late-usurper-bin /app/late-usurper
USER late

EXPOSE 2326

CMD ["/app/late-usurper"]

# ==============================================================================
# Stage 4h: Runtime Brogue - the late-brogue host (game served over SSH)
# ==============================================================================
# Owns everything the game needs: the from-source curses-only brogue binary
# (/opt/brogue, hangup-save patch applied), its runtime lib, and the writable
# playground (/var/lib/late-brogue; backed by a PVC in prod so the per-player
# save directories under players/ survive restarts). LATE_BROGUE_BIN defaults
# to /usr/games/brogue, LATE_BROGUE_DATA_DIR to that playground path.
FROM runtime-base AS runtime-brogue
USER root
# libncurses6: brogue's terminal build links plain -lncurses (pure-ASCII
# display, no wide-char calls). ncurses-term: the EXTENDED terminfo DB
# (alacritty, rxvt, st, etc.) so clients on those terminals get native
# terminfo rather than the xterm-256color fallback; terminals that ship their
# own terminfo (ghostty/kitty/wezterm) are covered by the host's TERM fallback
# in late-brogue (effective_term).
RUN apt-get update && apt-get install -y --no-install-recommends \
    libncurses6 \
    ncurses-term \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /var/lib/late-brogue && chown late:late /var/lib/late-brogue
COPY --from=brogue-build /opt/brogue /opt/brogue
RUN mkdir -p /usr/games && ln -sf /opt/brogue/brogue /usr/games/brogue
COPY --from=builder /app/late-brogue-bin /app/late-brogue
USER late

EXPOSE 2327

CMD ["/app/late-brogue"]
