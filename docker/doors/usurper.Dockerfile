# syntax=docker/dockerfile:1.4
#
# usurper door-game build image. The stage below moved verbatim from the root
# Dockerfile so the recipe rebuilds only when this file changes, not on every
# image build. Built and pushed by .github/workflows/usurper.yml as
# ghcr.io/mpiorowski/late-sh/door-usurper:0.25-r1; the root Dockerfile pins that
# image as its usurper-build stage. Bump the tag there on any recipe change.

ARG DEBIAN_VERSION=bookworm

# ==============================================================================
# Stage 0d: Usurper - Build the door game binary from verified upstream source
# ==============================================================================
# Usurper (the classic LORD-style BBS door, GPL-2.0-or-later, Rick Parrish's
# 32/64-bit Free Pascal port) runs in its own SSH host (late-usurper). The
# upstream CI cross-compiles from Windows with fpcupdeluxe, but the source
# builds cleanly with Debian's stock fpc using the same flags as upstream's
# build.ps1 (verified against the official release binary). We pin a source
# commit tarball + SHA-256 (`sha256sum -c`, fail-closed) rather than using the
# upstream "Development Build" zips, which are a moving pre-release tag.
#
# The game has no separate data tree: everything is resolved relative to the
# process working directory (DATA/, TEXT/, NODE/, SCORES/, DOCS/, USURPER.CFG).
# This stage assembles /opt/usurper: bin/ (USURPER.EXE + EDITOR.EXE) and seed/
# (the writable game-tree template the host copies into its data dir at boot).
# The world data files (MONSTER.DAT, NPCS.DAT, ...) are not distributed by
# upstream; they are generated here by scripting the EDITOR's Reset Game TUI
# (scripts/usurper_seed_data.py), with fail-closed asserts on the vital files.
# NPC generation is randomized, so the seed is not bit-reproducible; the world
# it defines is the stock one.
FROM debian:${DEBIAN_VERSION}-slim AS usurper-build

# Pinned to rickparrish/Usurper master (v0.25 development line, 2025-02-16
# build); update the commit + SHA-256 together.
ARG USURPER_COMMIT=7b04f7e5c50fc1f7cc3626186f10423994b171dd
ARG USURPER_URL=https://github.com/rickparrish/Usurper/archive/${USURPER_COMMIT}.tar.gz
ARG USURPER_SHA256=38f7ee61a2bb2d4b280e121aa4aeb64107c2c0d997a7d98d30174f393b18db0f
ARG USURPER_PREFIX=/opt/usurper

# fpc: the Free Pascal compiler (bookworm ships 3.2.2, same line as upstream's
# toolchain). python3-minimal drives the EDITOR reset on a PTY.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    fpc \
    python3-minimal \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
RUN curl -fsSL -o usurper.tar.gz "${USURPER_URL}" \
    && echo "${USURPER_SHA256}  usurper.tar.gz" | sha256sum -c - \
    && tar -xzf usurper.tar.gz \
    && mv "Usurper-${USURPER_COMMIT}" usurper \
    && rm usurper.tar.gz

WORKDIR /build/usurper
# The fpc invocation is upstream build.ps1's, retargeted to Linux x86-64
# (-Tlinux -Px86_64). The source contains Intel assembly, so local Compose pins
# only service-usurper to linux/amd64. TP compatibility mode (-Mtp), C-style
# operators + goto + inlining (-Scgi), O3, stripped + smartlinked. Separate obj
# dirs per program: the two share COMMON units compiled with different include
# paths.
RUN mkdir -p obj-usurper obj-editor bin \
    && fpc -B -Tlinux -Px86_64 -Mtp -Scgi -CX -O3 -Xs -XX -l -vewnibq \
        -FiSOURCE/USURPER -FiSOURCE/COMMON -Fiobj-usurper \
        -FuSOURCE/COMMON -FUobj-usurper -FEbin -obin/USURPER.EXE \
        SOURCE/USURPER/USURPER.PAS \
    && fpc -B -Tlinux -Px86_64 -Mtp -Scgi -CX -O3 -Xs -XX -l -vewnibq \
        -FiSOURCE/EDITOR -FiSOURCE/COMMON -Fiobj-editor \
        -FuSOURCE/COMMON -FUobj-editor -FEbin -obin/EDITOR.EXE \
        SOURCE/EDITOR/EDITOR.PAS \
    && test -x bin/USURPER.EXE \
    && test -x bin/EDITOR.EXE

# Assemble the seed game tree: the RELEASE assets the game reads at runtime
# (TEXT/ screens, DOCS/ shown by the in-game Instructions menu), the sample
# USURPER.CFG (game options; lines 1-2 are the displayed sysop/BBS names), and
# a minimal USURP.CTL naming the sysop "Late Sysop" - handles can't contain
# spaces and late/late_* are reserved arcade handles, so no player can ever
# match the sysop identity. UPGRADES/ (DOS-only tools) and the SDN-era
# metadata files are deliberately not shipped.
COPY scripts/usurper_seed_data.py /build/usurper_seed_data.py
RUN mkdir -p ${USURPER_PREFIX}/bin ${USURPER_PREFIX}/seed \
    && cp bin/USURPER.EXE bin/EDITOR.EXE ${USURPER_PREFIX}/bin/ \
    && cp -r RELEASE/TEXT RELEASE/DOCS ${USURPER_PREFIX}/seed/ \
    && cp RELEASE/COPYING ${USURPER_PREFIX}/seed/ \
    && cp RELEASE/SAMPLES/USURPER.CFG ${USURPER_PREFIX}/seed/USURPER.CFG \
    && sed -i '1s/.*/Late Sysop/;2s/.*/late.sh/' ${USURPER_PREFIX}/seed/USURPER.CFG \
    && printf 'SYSOPFIRST Late\nSYSOPLAST Sysop\nBBSNAME late.sh\n' > ${USURPER_PREFIX}/seed/USURP.CTL \
    && python3 /build/usurper_seed_data.py ${USURPER_PREFIX}/seed ${USURPER_PREFIX}/bin/EDITOR.EXE \
    && test -s ${USURPER_PREFIX}/seed/DATA/MONSTER.DAT \
    && test -s ${USURPER_PREFIX}/seed/DATA/NPCS.DAT \
    && test -s ${USURPER_PREFIX}/seed/DATA/GUARDS.DAT \
    && test -s ${USURPER_PREFIX}/seed/DATA/LEVELS.DAT \
    && test -s ${USURPER_PREFIX}/seed/DATA/TNAMES.DAT
