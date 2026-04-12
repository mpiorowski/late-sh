#!/usr/bin/env bash
#
# Admin-only: force the liquidsoap radio to a specific vibe (genre).
#
# Usage:
#   scripts/set_vibe.sh <lofi|classic|ambient|jazz|toggle>
#
# How it works:
#   Port-forwards the liquidsoap telnet service (1234) to a local port and
#   talks to it via bash's built-in /dev/tcp — no `nc` needed on either side.
#   The liquidsoap container image doesn't ship nc, and we don't assume it
#   locally either.
#
# Heads up:
#   The vote service will overwrite the vibe on its next tally tick, so run
#   this right after a tally if you want it to stick for a while.

set -euo pipefail

VIBE="${1:-}"

if [[ -z "${VIBE}" ]]; then
  echo "usage: $(basename "$0") <lofi|classic|ambient|jazz|toggle>" >&2
  exit 1
fi

case "${VIBE}" in
  lofi|classic|ambient|jazz)
    CMD="vibe.set ${VIBE}"
    ;;
  toggle)
    CMD="vibe.toggle"
    ;;
  *)
    echo "unknown vibe: ${VIBE}" >&2
    echo "valid: lofi, classic, ambient, jazz, toggle" >&2
    exit 1
    ;;
esac

SERVICE="${LIQUIDSOAP_SERVICE:-svc/liquidsoap-sv}"
KUBECTL="${KUBECTL:-kubectl}"
LOCAL_PORT="${LOCAL_PORT:-12340}"

if ! command -v "${KUBECTL}" >/dev/null 2>&1; then
  echo "${KUBECTL} is required" >&2
  exit 1
fi

echo "→ port-forwarding ${SERVICE} :${LOCAL_PORT} -> :1234"
"${KUBECTL}" port-forward "${SERVICE}" "${LOCAL_PORT}:1234" >/dev/null 2>&1 &
PF_PID=$!
trap 'kill "${PF_PID}" 2>/dev/null || true' EXIT

# Wait for the tunnel to be ready (bash /dev/tcp probe, up to ~5s).
for _ in $(seq 1 50); do
  if (exec 3<>"/dev/tcp/127.0.0.1/${LOCAL_PORT}") 2>/dev/null; then
    exec 3<&-
    exec 3>&-
    break
  fi
  sleep 0.1
done

echo "→ ${CMD}"
exec 3<>"/dev/tcp/127.0.0.1/${LOCAL_PORT}"
printf '%s\r\nquit\r\n' "${CMD}" >&3
# Drain server response until it closes the connection.
while IFS= read -r line <&3; do
  # liquidsoap terminates responses with "END"; print everything until EOF.
  printf '%s\n' "${line}"
done
exec 3<&-
exec 3>&-
