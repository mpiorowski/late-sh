# syntax=docker/dockerfile:1
#
# Vintage Story dedicated server for linux/arm64: agent-2 is a Hetzner CAX21
# (infra/vintagestory.tf). Anego publishes the server only as linux-x64; their
# official ARM recipe (github.com/anegostudios/VintagestoryServerArm64) takes
# that tarball, deletes the x64 launcher and Lib/, and drops in an arm64 build
# of both. The game code itself (VintagestoryLib, the API, assets, built-in
# mods) is plain IL and runs unchanged. One ARM release covers a whole minor
# line: 1.22.0 serves every 1.22.x.
#
# The fetch stage runs on the build host's own platform and only downloads and
# unpacks, so CI builds this on an x86 runner with no emulation. The runtime
# stage has no RUN step for the same reason.
#
# Bumping the game: set VS_VERSION and VS_SERVER_MD5 from
# https://api.vintagestory.at/stable.json (`linuxserver`), bump the ARM release
# only for a new minor line, then bump `vintagestory_image` in
# infra/defaults.tf (late-ssh/src/app/door/vintagestory/CONTEXT.md, Upgrading).

FROM --platform=$BUILDPLATFORM debian:trixie-slim AS fetch

ARG VS_VERSION=1.22.7
ARG VS_SERVER_MD5=3031212903b1c283084d5fe0cda6bce6
ARG VS_ARM64_RELEASE=1.22.0
ARG VS_ARM64_SHA256=191c7f3a58a89a47843e7ab8d1fb16d51f3030f7a6cead639c7c48a907679bfa

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/vintagestory
RUN curl -fsSL -o /tmp/server.tgz \
        "https://cdn.vintagestory.at/gamefiles/stable/vs_server_linux-x64_${VS_VERSION}.tar.gz" \
    && echo "${VS_SERVER_MD5}  /tmp/server.tgz" | md5sum -c - \
    && curl -fsSL -o /tmp/arm64.tgz \
        "https://github.com/anegostudios/VintagestoryServerArm64/releases/download/${VS_ARM64_RELEASE}/vs_server_linux-arm64_${VS_ARM64_RELEASE}.tar.gz" \
    && echo "${VS_ARM64_SHA256}  /tmp/arm64.tgz" | sha256sum -c - \
    && tar xzf /tmp/server.tgz \
    && rm -rf VintagestoryServer VintagestoryServer.deps.json VintagestoryServer.dll \
        VintagestoryServer.pdb VintagestoryServer.runtimeconfig.json Lib \
    && tar xzf /tmp/arm64.tgz \
    && rm /tmp/server.tgz /tmp/arm64.tgz \
    && test -f VintagestoryServer.dll \
    && test -f Lib/libe_sqlite3.so

FROM mcr.microsoft.com/dotnet/runtime:10.0

COPY --from=fetch /opt/vintagestory /opt/vintagestory
WORKDIR /opt/vintagestory

# The base image's own non-root user (uid 1654). The world lives on the
# volume mounted at /data, passed as --dataPath by the pod spec.
USER app

# The server traps SIGTERM itself: it saves the world and exits 0, so no
# wrapper sits between it and the kubelet. Console commands arrive on stdin.
ENTRYPOINT ["dotnet", "/opt/vintagestory/VintagestoryServer.dll"]
