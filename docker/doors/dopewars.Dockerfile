# syntax=docker/dockerfile:1.4
#
# dopewars door-game build image. The stage below moved verbatim from the root
# Dockerfile so the recipe rebuilds only when this file changes, not on every
# image build. Built and pushed by .github/workflows/dopewars.yml as
# ghcr.io/mpiorowski/late-sh/door-dopewars:1.6.2-r1; the root Dockerfile pins that
# image as its dopewars-build stage. Bump the tag there on any recipe change.

ARG DEBIAN_VERSION=bookworm

# ==============================================================================
# Stage 0b: dopewars - Build the door game binary from verified upstream source
# ==============================================================================
# Like NetHack, dopewars runs in its own SSH host (late-dopewars); this stage
# builds the binary, which is copied into runtime-dopewars for prod (and base for
# dev-dopewars). We build the curses client terminal-only (no GTK/SDL/sound) from
# the verified 1.6.2 release tarball: runtime deps are just glib2 + ncursesw (+
# libcurl, pulled in by the optional metaserver client). The binary is
# self-contained -- drug/location data is compiled in, no data dir -- and is NOT
# setgid, so it honors the shared `-f` high-score path the host passes.
#
# The tarball SHA-256 is verified BEFORE the build (downloaded + hashed 2026-06-30);
# `sha256sum -c` fails the build closed on any mismatch.
FROM debian:${DEBIAN_VERSION}-slim AS dopewars-build

ARG DOPEWARS_VERSION=1.6.2
ARG DOPEWARS_TARBALL=dopewars-1.6.2.tar.gz
ARG DOPEWARS_URL=https://downloads.sourceforge.net/project/dopewars/dopewars/1.6.2/dopewars-1.6.2.tar.gz
ARG DOPEWARS_SHA256=623b9d1d4d576f8b1155150975308861c4ec23a78f9cc2b24913b022764eaae1

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    build-essential \
    pkg-config \
    libglib2.0-dev \
    libncursesw5-dev \
    libcurl4-openssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
RUN curl -fsSL -o "${DOPEWARS_TARBALL}" "${DOPEWARS_URL}" \
    && echo "${DOPEWARS_SHA256}  ${DOPEWARS_TARBALL}" | sha256sum -c - \
    && tar -xzf "${DOPEWARS_TARBALL}" \
    && rm "${DOPEWARS_TARBALL}"

# Terminal-only build (GUI/server/sound disabled). The release Makefile drops
# $(CURSES_LIBS) from dopewars_LDADD when the GTK client is disabled, so the curses
# symbols are injected via the trailing $(LIBS) on the link line (LIBS=-lncursesw).
# Copy the finished binary to a version-independent path for the COPY --from below.
WORKDIR /build/dopewars-${DOPEWARS_VERSION}
RUN ./configure --disable-gui-client --disable-gui-server --enable-curses-client \
    && make LIBS="-lncursesw" \
    && test -x src/dopewars \
    && cp src/dopewars /dopewars
