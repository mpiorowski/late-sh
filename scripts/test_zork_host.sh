#!/usr/bin/env bash
# Full client -> SSH -> PTY -> Frotz test against the non-root runtime image.
# Build first: docker build --platform linux/amd64 --target runtime-zork -t late-zork:local .
set -euo pipefail
IMAGE="${1:-late-zork:local}"
NAME="late-zork-test-$$"
VOLUME="$NAME-data"
cleanup() {
    if [ "$?" -ne 0 ]; then docker logs "$NAME" >&2 2>/dev/null || true; fi
    docker rm -f "$NAME" >/dev/null 2>&1 || true
    docker volume rm "$VOLUME" >/dev/null 2>&1 || true
}
trap cleanup EXIT
docker volume create "$VOLUME" >/dev/null
docker run --rm --user root -v "$VOLUME:/var/lib/late-zork" --entrypoint sh "$IMAGE" \
    -ec 'chown late:late /var/lib/late-zork'
docker run -d --name "$NAME" -p 127.0.0.1::2331 \
    -v "$VOLUME:/var/lib/late-zork" -e LATE_ZORK_SECRET=zork-integration-test "$IMAGE" >/dev/null
PORT=$(docker port "$NAME" 2331/tcp)
export LATE_ZORK_TEST_PORT="${PORT##*:}"
export LATE_ZORK_TEST_CONTAINER="$NAME"
# Host startup is bounded; no dependency on a fixed local port.
for _ in $(seq 1 50); do
    if (exec 3<>"/dev/tcp/127.0.0.1/$LATE_ZORK_TEST_PORT") 2>/dev/null; then break; fi
    sleep 0.1
done
cargo nextest run -p late-ssh --run-ignored only -E 'test(real_frotz_trilogy_roundtrip)'
cargo nextest run -p late-ssh --run-ignored only -E 'test(real_frotz_host_restart_preserves_six_slots)'
