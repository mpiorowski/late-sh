#!/usr/bin/env bash
# Smoke test for the door-codekeep image. $1 = locally loaded image tag,
# $2 = the published door tag (e.g. 1.0.9-r1). The stage itself installs from
# the frozen lock and asserts the exact version; run the shipped wrapper as a
# smoke test too. The upstream version is the tag minus the -rN recipe suffix.
set -euo pipefail
IMAGE="$1"
TAG="$2"

docker run --rm "$IMAGE" /usr/local/bin/codekeep --version | grep -qx "${TAG%-r*}"
