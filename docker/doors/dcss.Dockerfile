# syntax=docker/dockerfile:1.4
#
# dcss door-game build image. The stage below moved verbatim from the root
# Dockerfile so the recipe rebuilds only when this file changes, not on every
# image build. Built and pushed by .github/workflows/dcss.yml as
# ghcr.io/mpiorowski/late-sh/door-dcss:0.34.1-r1; the root Dockerfile pins that
# image as its dcss-build stage. Bump the tag there on any recipe change.

ARG DEBIAN_VERSION=bookworm

# ==============================================================================
# Stage 0c: DCSS - Build the door game binary from verified upstream source
# ==============================================================================
# Like NetHack, Dungeon Crawl Stone Soup runs in its own SSH host (late-dcss);
# this stage builds the console (non-tiles) binary, which is copied into
# runtime-dcss for prod (and base for dev-dcss). We build from the official
# release tarball rather than installing the distro "crawl" package because the
# Debian package lags well behind upstream (bookworm ships 0.29; we want 0.34).
#
# The tarball SHA-256 is verified BEFORE the build (downloaded + hashed
# 2026-07-18 from the GitHub release); `sha256sum -c` fails the build closed on
# any mismatch. Build recipe follows the release's own INSTALL.md ("Installing
# For All Users"): `make install prefix=...` produces the console build by
# default (tiles needs an explicit TILES=y, which we do not pass) and bakes
# DATADIR=$prefix/data into the binary. SAVEDIR stays the default `~/.crawl`,
# so per-player saves land under the child's HOME (the host's
# LATE_DCSS_DATA_DIR playground), keyed by the `-name` the host passes.
FROM debian:${DEBIAN_VERSION}-slim AS dcss-build

ARG DCSS_VERSION=0.34.1
ARG DCSS_TARBALL=stone_soup-0.34.1.tar.xz
ARG DCSS_URL=https://github.com/crawl/crawl/releases/download/0.34.1/stone_soup-0.34.1.tar.xz
ARG DCSS_SHA256=473b9cdc16be0b537ac11e43c6c77db4b290000e4a17f72a842eba59c6b7be2a
# Everything (binary + read-only data) installs under this prefix; the runtime
# stages copy the whole tree and symlink the binary to /usr/games/crawl (the
# LATE_DCSS_BIN default).
ARG DCSS_PREFIX=/opt/dcss

# The console-build dependency list from INSTALL.md (Ubuntu/Debian section),
# minus the tiles-only SDL/freetype set. xz-utils unpacks the .tar.xz release.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    xz-utils \
    build-essential \
    bison \
    flex \
    pkg-config \
    libncursesw5-dev \
    liblua5.4-dev \
    libsqlite3-dev \
    libz-dev \
    python3-yaml \
    python-is-python3 \
    binutils-gold \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
RUN curl -fsSL -o "${DCSS_TARBALL}" "${DCSS_URL}" \
    && echo "${DCSS_SHA256}  ${DCSS_TARBALL}" | sha256sum -c - \
    && tar -xJf "${DCSS_TARBALL}" \
    && rm "${DCSS_TARBALL}"

WORKDIR /build/stone_soup-${DCSS_VERSION}/source
# With a bare `prefix`, crawl's Makefile installs the binary to $prefix/bin and
# the read-only data tree to $prefix/data (NOT the $prefix/share/crawl the
# INSTALL.md mentions -- verified from the actual install log), baking that
# DATADIR into the binary. The asserts pin both landing spots.
#
# NOWIZARD=y compiles OUT wizard (cheat) mode, which local builds enable by
# default; the Makefile's own comment says to set it "if you have untrusted"
# users, which a hosted door is. The -version grep fails the build closed if a
# version bump ever re-enables it (-DWIZARD would reappear in the CFLAGS line).
RUN make -j"$(nproc)" prefix=${DCSS_PREFIX} NOWIZARD=y install \
    && test -x ${DCSS_PREFIX}/bin/crawl \
    && test -d ${DCSS_PREFIX}/data/dat \
    && ! ${DCSS_PREFIX}/bin/crawl -version | grep -q -- -DWIZARD
