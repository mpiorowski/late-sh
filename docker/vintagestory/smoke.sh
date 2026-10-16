#!/usr/bin/env bash
# Smoke test for the vintagestory image. $1 = locally loaded image tag; runs on
# an arm64 host. Boots a throwaway world, runs a console command over stdin
# the way operators do (`kubectl attach`), then stops the container and
# requires the SIGTERM save: a server that dies without saving loses the
# world on every pod restart.
set -euo pipefail
IMAGE="$1"
NAME="vs-smoke"

data=$(mktemp -d)
chmod 777 "$data"

# The server writes the world as the image's uid 1654, which the calling user
# cannot delete, so the files go through a root container. Cleanup never
# decides the result.
cleanup() {
  docker rm -f "$NAME" >/dev/null 2>&1 || true
  docker run --rm --user 0 -v "$data:/data" --entrypoint sh "$IMAGE" \
    -c 'rm -rf /data/* /data/.[!.]*' >/dev/null 2>&1 || true
  rmdir "$data" 2>/dev/null || true
}
trap cleanup EXIT

docker run -d -i --name "$NAME" -v "$data:/data" "$IMAGE" \
  --dataPath /data \
  --withconfig="{ WhitelistMode: 'On', VerifyPlayerAuth: true, AdvertiseServer: false }"

wait_for_log() {
  local pattern="$1"
  for _ in $(seq 1 180); do
    if docker logs "$NAME" 2>&1 | grep -q -- "$pattern"; then
      return 0
    fi
    if [ "$(docker inspect -f '{{.State.Running}}' "$NAME")" != "true" ]; then
      docker logs "$NAME" 2>&1 | tail -40
      echo "::error::server exited while waiting for: $pattern"
      exit 1
    fi
    sleep 2
  done
  docker logs "$NAME" 2>&1 | tail -40
  echo "::error::timed out waiting for: $pattern"
  exit 1
}

wait_for_log "Dedicated Server now running on Port 42420"

# `docker attach` streams the container's output until the container exits,
# even after its stdin closes, so it is cut off once the line is delivered.
echo "/stats" | timeout 10 docker attach "$NAME" >/dev/null 2>&1 || true
wait_for_log "Handling Console Command /stats"

docker stop -t 120 "$NAME" >/dev/null
docker logs "$NAME" 2>&1 | grep -q "World saved!" || {
  docker logs "$NAME" 2>&1 | tail -40
  echo "::error::no world save on SIGTERM"
  exit 1
}
test "$(docker inspect -f '{{.State.ExitCode}}' "$NAME")" = "0"
echo "boot, console, and SIGTERM save OK"
