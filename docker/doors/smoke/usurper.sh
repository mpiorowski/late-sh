#!/usr/bin/env bash
# Smoke test for the door-usurper image. $1 = locally loaded image tag.
# The stage itself already asserts the binaries and the generated seed world
# files fail-closed; we re-prove them here.
set -euo pipefail
IMAGE="$1"

docker run --rm "$IMAGE" sh -c '
  test -x /opt/usurper/bin/USURPER.EXE &&
  test -x /opt/usurper/bin/EDITOR.EXE &&
  test -s /opt/usurper/seed/DATA/MONSTER.DAT &&
  test -s /opt/usurper/seed/DATA/NPCS.DAT &&
  test -s /opt/usurper/seed/USURPER.CFG &&
  test -s /opt/usurper/seed/TEXT/MAINMENU.ANS &&
  test -s /opt/usurper/seed/COPYING'
