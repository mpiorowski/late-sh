#!/usr/bin/env bash
# Smoke test for the door-nethack image. $1 = locally loaded image tag.
# The stage itself already asserts the binary lands in HACKDIR and that save/
# was seeded under VAR_PLAYGROUND; we re-prove both at runtime.
set -euo pipefail
IMAGE="$1"

docker run --rm "$IMAGE" /var/games/nethack/nethack --version

# Prove both windowports shipped in the installed binary: tty (the default) and
# curses (opt-in via OPTIONS=windowtype:curses, the only port whose input layer
# decodes arrow keys). Each port registers in winchoices[] by an exact name
# string (WPID stringizes it into .rodata), so an exact-line strings match is a
# reliable witness; the build stage already asserts the same via nm on the
# pre-install binary. `-n 3` because `strings` defaults to 4-character runs
# and would never emit the standalone `tty`.
docker run --rm "$IMAGE" sh -c '
  set -eu
  strings -n 3 /var/games/nethack/nethack | grep -qx tty
  strings -n 3 /var/games/nethack/nethack | grep -qx curses
'

# Prove WANT_DEFAULT=tty actually stuck as DEFAULT_WINDOW_SYS, without naming
# any internal strings: with no usable terminal each port fails init with its
# own message, so a default launch must fail exactly like a forced-tty launch
# and differently from a forced-curses one. The second test also catches the
# degenerate case where both ports fail identically and the probe proves
# nothing; a red there means the probe needs rework, not that the image is bad.
#
# Two things must hold for the ports to reach their own init failure at all,
# or every probe stalls on a pre-init "Hit return to continue:" prompt (stdin
# is /dev/null, so it only ends when `timeout` fires) with identical output:
#   - the playground must be writable by the probe's unprivileged uid, or
#     check_recordfile() warns about the scoreboard before any port is up,
#     hence the tmpfs, same as the -s test below;
#   - the default probe must leave NETHACKOPTIONS unset rather than empty,
#     since an empty value parses as "Empty statement" and warns the same way.
probe() {
  docker run --rm --user 65534:65534 \
    --tmpfs /var/games/nethack-var:rw,mode=1777 \
    -e TERM= "$@" "$IMAGE" \
    sh -c 'timeout 15 /var/games/nethack/nethack </dev/null 2>&1 || true'
}
default_out="$(probe)"
tty_out="$(probe -e NETHACKOPTIONS=windowtype:tty)"
curses_out="$(probe -e NETHACKOPTIONS=windowtype:curses)"
test "$default_out" = "$tty_out"
test "$default_out" != "$curses_out"

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
