# syntax=docker/dockerfile:1.4
#
# brogue door-game build image. The stage below moved verbatim from the root
# Dockerfile so the recipe rebuilds only when this file changes, not on every
# image build. Built and pushed by .github/workflows/brogue.yml as
# ghcr.io/mpiorowski/late-sh/door-brogue:1.15.1-r3; the root Dockerfile pins that
# image as its brogue-build stage. Bump the tag there on any recipe change.

ARG DEBIAN_VERSION=bookworm

# ==============================================================================
# Stage 0e: Brogue CE - Build the door game binary from verified upstream source
# ==============================================================================
# Brogue CE runs in its own SSH host (late-brogue); this stage builds the
# curses-only binary (TERMINAL=YES GRAPHICS=NO: no SDL, no tiles; RELEASE=YES
# drops the "-dev" version suffix, which brogue bakes into save filenames and
# save compatibility). The tarball SHA-256 is verified BEFORE the build
# (downloaded + hashed 2026-07-21 from the GitHub tag archive); `sha256sum -c`
# fails the build closed on any mismatch.
#
# Two source patches, each asserted fail-closed after applying (re-verify both
# on Brogue CE version bumps):
# - scripts/brogue_hangup_save.patch: upstream's curses build dies unsaved on
#   SIGHUP (only the SDL window-close path auto-saves), so the patch installs a
#   hangup handler running the same quitImmediately save path. The late-brogue
#   host relies on it for teardown saves; the grep asserts it landed.
# - scripts/brogue_victory_log.patch: 1.15.1's victory path has the run-history
#   condition inverted (mode != EASY && mode != NORMAL), so normal victories are
#   never logged while wizard victories are. The late.sh award pipe reads the
#   run history for the Escaped/Mastered badge pair, so the fix matters twice
#   over. GAME_MODE_NORMAL appears exactly once in RogueMain.c (the buggy
#   line), so asserting its absence proves the patch landed.
#
# scripts/brogue_verify_isolation.sh then asserts the upstream properties that
# per-player save isolation rests on (no path can reach a filename, files keep a
# known suffix, the file browser lists only the cwd, nothing spawns a process).
# Players are separated by their working directory alone, and only upstream's
# own input filter makes that a wall, so a CE bump that relaxes it must fail the
# build here: no Rust test can see it. Details in the brogue door's CONTEXT.md
# section 4.
#
# The terminal build reads no data files (DATADIR only matters for tiles), and
# opens every player file relative to its working directory; the host gives
# each player their own cwd under LATE_BROGUE_DATA_DIR.
FROM debian:${DEBIAN_VERSION}-slim AS brogue-build

ARG BROGUE_VERSION=1.15.1
ARG BROGUE_TARBALL=BrogueCE-1.15.1.tar.gz
ARG BROGUE_URL=https://github.com/tmewett/BrogueCE/archive/refs/tags/v1.15.1.tar.gz
ARG BROGUE_SHA256=2abc186c5327342cb9ad7e45d41096ab10797d5ba76dcac843824ac2a0bfb3ac

# diffutils: the Makefile uses cmp to keep generated headers fresh. patch:
# applies the hangup-save patch. libncurses-dev: the terminal build links
# plain -lncurses (the display is pure ASCII, no wide-char calls).
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    build-essential \
    diffutils \
    patch \
    libncurses-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY scripts/brogue_hangup_save.patch /build/brogue_hangup_save.patch
COPY scripts/brogue_victory_log.patch /build/brogue_victory_log.patch
COPY scripts/brogue_verify_isolation.sh /build/brogue_verify_isolation.sh
RUN curl -fsSL -o "${BROGUE_TARBALL}" "${BROGUE_URL}" \
    && echo "${BROGUE_SHA256}  ${BROGUE_TARBALL}" | sha256sum -c - \
    && tar -xzf "${BROGUE_TARBALL}" \
    && rm "${BROGUE_TARBALL}"

WORKDIR /build/BrogueCE-${BROGUE_VERSION}
RUN patch -p1 < /build/brogue_hangup_save.patch \
    && grep -q handleHangup src/platform/curses-platform.c \
    && patch -p1 < /build/brogue_victory_log.patch \
    && ! grep -q GAME_MODE_NORMAL src/brogue/RogueMain.c \
    && sh /build/brogue_verify_isolation.sh . \
    && make -j"$(nproc)" bin/brogue TERMINAL=YES GRAPHICS=NO RELEASE=YES \
    && test -x bin/brogue \
    && ./bin/brogue --version | grep -qx "Brogue version: CE ${BROGUE_VERSION}" \
    && mkdir -p /opt/brogue \
    && cp bin/brogue /opt/brogue/brogue
