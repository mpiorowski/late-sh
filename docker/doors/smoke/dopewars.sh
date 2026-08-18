#!/usr/bin/env bash
# Smoke test for the door-dopewars image. $1 = locally loaded image tag.
# The stage itself already asserts the binary lands at /dopewars; we re-prove
# it runs and re-check the setgid invariant here.
set -euo pipefail
IMAGE="$1"

docker run --rm "$IMAGE" /dopewars --version

# Prove the setgid invariant holds: dopewars silently refuses a user `-f`
# score path under setgid, which would break the per-session score file
# without erroring loudly (see CONTEXT.md §5).
docker run --rm "$IMAGE" sh -c '
  set -e
  mode=$(stat -c "%a" /dopewars)
  if [ -g /dopewars ]; then
    echo "::error::dopewars binary is setgid (mode $mode)"
    exit 1
  fi
  echo "mode $mode OK (not setgid)"
'
