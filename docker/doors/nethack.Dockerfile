# syntax=docker/dockerfile:1.4
#
# nethack door-game build image. The stage below moved verbatim from the root
# Dockerfile so the recipe rebuilds only when this file changes, not on every
# image build. Built and pushed by .github/workflows/nethack.yml as
# ghcr.io/mpiorowski/late-sh/door-nethack:5.0.0-r1; the root Dockerfile pins that
# image as its nethack-build stage. Bump the tag there on any recipe change.

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
