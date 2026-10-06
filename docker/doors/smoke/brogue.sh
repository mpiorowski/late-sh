#!/usr/bin/env bash
# Smoke test for the door-brogue image. $1 = locally loaded image tag.
# The stage itself already asserts the hangup-save patch applied and the
# version string; we re-prove the binary actually runs here.
set -euo pipefail
IMAGE="$1"

docker run --rm "$IMAGE" /opt/brogue/brogue --version
