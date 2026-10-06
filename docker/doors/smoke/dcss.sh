#!/usr/bin/env bash
# Smoke test for the door-dcss image. $1 = locally loaded image tag.
# The stage itself already asserts the binary and data tree land under
# /opt/dcss; we re-prove the binary actually runs (-version resolves the baked
# DATADIR and version info).
set -euo pipefail
IMAGE="$1"

docker run --rm "$IMAGE" /opt/dcss/bin/crawl -version
