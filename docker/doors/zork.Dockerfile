# syntax=docker/dockerfile:1.4
# Frotz 042d7bc (2.56pre) plus late.sh's GPL door changes; MIT story pins and
# hashes live in assets/zork/PROVENANCE.md and SHA256SUMS. Bump door-zork's
# root Dockerfile tag whenever this recipe, the source, or the stories change.
ARG DEBIAN_VERSION=bookworm
FROM debian:${DEBIAN_VERSION}-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential libncurses-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY vendor/frotz vendor/frotz
COPY assets/zork assets/zork
COPY docker/doors/zork.Dockerfile /usr/share/doc/frotz/zork.Dockerfile
# Preserve the complete corresponding source before the build generates files.
RUN mkdir -p /usr/share/doc/frotz/source \
    && cp -a vendor/frotz/. /usr/share/doc/frotz/source/ \
    && cp vendor/frotz/COPYING /usr/share/doc/frotz/COPYING \
    && cd assets/zork && sha256sum -c SHA256SUMS
RUN make -C vendor/frotz curses SOUND_TYPE=none ZORK_DOOR=1 -j"$(nproc)" \
    && install -D -m 0755 vendor/frotz/frotz /usr/games/frotz

FROM debian:${DEBIAN_VERSION}-slim AS zork-build
RUN apt-get update && apt-get install -y --no-install-recommends \
    libncursesw6 ncurses-term python3 tmux ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --user-group late
COPY --from=ghcr.io/astral-sh/uv:0.12.24 /uv /usr/local/bin/uv
COPY --from=build /usr/games/frotz /usr/games/frotz
COPY --from=build /build/assets/zork /usr/share/late-zork
COPY --from=build /usr/share/doc/frotz /usr/share/doc/frotz
ENV LATE_ZORK_TEST_BIN=/usr/games/frotz \
    LATE_ZORK_TEST_STORIES=/usr/share/late-zork
USER late
WORKDIR /home/late
