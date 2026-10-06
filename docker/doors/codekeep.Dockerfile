# syntax=docker/dockerfile:1.4
#
# CodeKeep door-game runtime image. Built and pushed by
# .github/workflows/codekeep.yml as
# ghcr.io/mpiorowski/late-sh/door-codekeep:1.0.9-r1; the root Dockerfile pins
# that image as its codekeep-build stage. Bump the tag there on any recipe or
# upstream package change.

# ==============================================================================
# Stage 0f: CodeKeep - Resolve the exact npm package with Bun
# ==============================================================================
# CodeKeep is an npm-published Ink TUI. The exact package and every transitive
# dependency are resolved from the checked-in Bun lock. Runtime never uses
# bunx or reaches the registry.
FROM oven/bun:1.3.10-debian AS codekeep-build

WORKDIR /opt/codekeep
COPY docker/codekeep/package.json docker/codekeep/bun.lock ./
RUN bun install --production --frozen-lockfile \
    && bun node_modules/codekeep/dist/index.js --version | grep -qx '1.0.9'

COPY docker/codekeep/codekeep /usr/local/bin/codekeep
RUN chmod 0755 /usr/local/bin/codekeep
