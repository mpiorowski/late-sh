#!/usr/bin/env bash
# Smoke test for the door-nethack image. $1 = locally loaded image tag.
# The stage itself already asserts the binary lands in HACKDIR and that save/
# was seeded under VAR_PLAYGROUND; we re-prove both at runtime.
set -euo pipefail
IMAGE="$1"

docker run --rm "$IMAGE" /var/games/nethack/nethack --version

# Prove the A1 split actually compiled in and the install seeded save/.
# GCC lowers the constant-path copy into immediate stores, so `strings`
# cannot reliably find VAR_PLAYGROUND in the optimized binary. Instead,
# run the score reader as an unprivileged user with a temporary playground
# and verify that it reads record from there. If it falls back to HACKDIR,
# the temporary record's access time remains unchanged.
docker run --rm "$IMAGE" test -d /var/games/nethack-var/save
docker run --rm \
  --user 65534:65534 \
  --tmpfs /var/games/nethack-var:rw,mode=1777,strictatime \
  "$IMAGE" sh -c '
  set -eu
  record=/var/games/nethack-var/record
  : > "$record"
  touch -a -d @1 "$record"
  before=$(stat -c %X "$record")
  /var/games/nethack/nethack -s >/dev/null
  after=$(stat -c %X "$record")
  test "$after" -gt "$before"
'
