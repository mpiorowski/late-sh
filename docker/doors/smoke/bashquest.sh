#!/usr/bin/env bash
# Smoke test for the door-bashquest image. $1 = locally loaded image tag.
# The stage itself already fails closed on a checksum mismatch; this re-proves
# the fetched script is present, executable, and syntactically valid.
set -euo pipefail
IMAGE="$1"

docker run --rm "$IMAGE" sh -c '
  set -e
  [ -x /bashquest.sh ] || { echo "::error::bashquest.sh is not executable"; exit 1; }
  bash -n /bashquest.sh
  echo "bashquest.sh present, executable, and syntactically valid"
'
