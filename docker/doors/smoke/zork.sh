#!/usr/bin/env bash
# Real curses/PTY acceptance against the non-root, independently built asset.
set -euo pipefail
IMAGE="$1"
docker run --rm "$IMAGE" sh -ec '
  test "$(id -u)" -ne 0
  cd /usr/share/late-zork
  sha256sum -c SHA256SUMS
  test -s /usr/share/doc/frotz/COPYING
  test -s /usr/share/doc/frotz/source/src/curses/ux_door.c
  test -s /usr/share/doc/frotz/zork.Dockerfile
  uv run --no-project /usr/share/doc/frotz/source/src/curses/ux_door_test.py
'
