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
    # The graceful door teardown (late-nethack host.rs) relies on NetHack's SIGHUP
    # hangup-save: on a client disconnect or host SIGTERM the host SIGHUPs the
    # child so NetHack writes a recoverable save AND releases its getlock slot,
    # instead of leaking the slot via SIGKILL (leaks accumulate until all
    # MAXPLAYERS slots are gone, wedging the whole door for everyone).
    # SAFERHANGUP defers the hangup to a safe point in the command loop rather than
    # saving from inside the signal handler. It ships enabled by default; the sed
    # re-enables the single-line-commented form if a version bump flips that, then
    # the grep asserts it is active. Fail-closed; re-verify on NetHack bumps.
    && sed -i 's|^/\* #define SAFERHANGUP \*/|#define SAFERHANGUP|' include/unixconf.h \
    && grep -qE '^#define SAFERHANGUP\b' include/unixconf.h \
    && cd sys/unix && sh setup.sh hints/linux.500 && cd ../.. \
    && make fetch-Lua \
    && make PREFIX=${NETHACK_PREFIX} HACKDIR=${NETHACK_HACKDIR} VARDIR=${NETHACK_VAR_PLAYGROUND} GAMEUID=root GAMEGRP=games all \
    && make PREFIX=${NETHACK_PREFIX} HACKDIR=${NETHACK_HACKDIR} VARDIR=${NETHACK_VAR_PLAYGROUND} GAMEUID=root GAMEGRP=games install \
    # Raise the concurrent-game cap. sysconf ships MAXPLAYERS=10; each value is
    # one live getlock slot, and once every slot is taken the whole door wedges
    # ("Too many hacks running now"), so size it up from the stock default.
    # NetHack hard-caps MAXPLAYERS at 25 (src/sys.c: values above it are rejected
    # at startup with "Illegal value in MAXPLAYERS", which sysconf parsing does
    # NOT fail closed on -- it just ignores the line), so 25 is the ceiling; it
    # fits the host pod's 1Gi budget (~10-20MB/game) with room to spare. The grep
    # only asserts the file was rewritten -- the 25 cap itself is upstream's.
    && sed -i 's/^MAXPLAYERS=.*/MAXPLAYERS=25/' ${NETHACK_HACKDIR}/sysconf \
    && grep -qx 'MAXPLAYERS=25' ${NETHACK_HACKDIR}/sysconf \
    # `make install` writes sysconf as 0600 root. HACKDIR is read-only at runtime
    # and the host runs as the unprivileged `late` user, which must READ sysconf at
    # startup -- otherwise nethack aborts with "Unable to open SYSCF_FILE." Make it
    # world-readable (it holds only non-secret game sysconf). This is why the door
    # worked in dev (runs as root) but failed in the prod pod (runs as late).
    && chmod 0644 ${NETHACK_HACKDIR}/sysconf \
    && test -x ${NETHACK_HACKDIR}/nethack \
    && [ "$(stat -c '%a' ${NETHACK_HACKDIR}/sysconf)" = "644" ] \
    && test -d ${NETHACK_VAR_PLAYGROUND}/save

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
# One source patch (scripts/brogue_hangup_save.patch): upstream's curses build
# dies unsaved on SIGHUP (only the SDL window-close path auto-saves), so the
# patch installs a hangup handler running the same quitImmediately save path.
# The late-brogue host relies on it for teardown saves; the grep asserts it
# landed, fail-closed. Re-verify the patch on Brogue CE version bumps.
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
RUN curl -fsSL -o "${BROGUE_TARBALL}" "${BROGUE_URL}" \
    && echo "${BROGUE_SHA256}  ${BROGUE_TARBALL}" | sha256sum -c - \
    && tar -xzf "${BROGUE_TARBALL}" \
    && rm "${BROGUE_TARBALL}"

WORKDIR /build/BrogueCE-${BROGUE_VERSION}
RUN patch -p1 < /build/brogue_hangup_save.patch \
    && grep -q handleHangup src/platform/curses-platform.c \
    && make -j"$(nproc)" bin/brogue TERMINAL=YES GRAPHICS=NO RELEASE=YES \
    && test -x bin/brogue \
    && ./bin/brogue --version | grep -qx "Brogue version: CE ${BROGUE_VERSION}" \
    && mkdir -p /opt/brogue \
    && cp bin/brogue /opt/brogue/brogue


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
