#!/usr/bin/env bash
#
# Add players to the production Vintage Story whitelist, effective immediately.
# Sends `/player NAME whitelist on` to the server console over `kubectl attach`,
# one name at a time, and reads the answer back from the pod log. The server
# resolves each name with Anego's auth server, so a typo fails here instead of
# whitelisting nobody. No restart; Playerdata/playerswhitelisted.json on the
# volume is rewritten on the spot and survives restarts and world resets.
#
# Usage:
#   scripts/vintagestory_whitelist_add.sh BearByte
#   scripts/vintagestory_whitelist_add.sh name1 name2 name3
#
# There is no seed list in Terraform or GitHub; the volume is the only copy.
#
# Optional env (same conventions as scripts/connect_db.sh):
#   KUBECTL=kubectl  KUBE_CONTEXT=<ctx>  KUBE_NAMESPACE=default

set -euo pipefail

usage() {
  sed -n '2,17p' "$0" >&2
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ $# -eq 0 ]]; then
  usage
  exit 1
fi

KUBECTL="${KUBECTL:-kubectl}"
KUBE_NAMESPACE="${KUBE_NAMESPACE:-default}"
DEPLOYMENT="deploy/vintagestory"
ANSWER_TIMEOUT_SECONDS=30

KUBECTL_ARGS=()
if [[ -n "${KUBE_CONTEXT:-}" ]]; then
  KUBECTL_ARGS+=(--context "${KUBE_CONTEXT}")
fi

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "$1 is required" >&2; exit 1; }
}

kube() {
  "${KUBECTL}" "${KUBECTL_ARGS[@]}" -n "${KUBE_NAMESPACE}" "$@"
}

console() {
  echo "$1" | kube attach -i "${DEPLOYMENT}" >/dev/null 2>&1
}

# Prints the server's answer to the last `/player $1 whitelist on` in the log
# since $2, or nothing while it has not answered yet.
answer_since() {
  local name="$1"
  local since="$2"

  kube logs "${DEPLOYMENT}" --since-time="${since}" | awk -v cmd="Handling Console Command /player ${name} whitelist on" '
    index($0, cmd) { answer = ""; seen = 1; next }
    seen && answer == "" && /\[Server Notification\]/ { answer = $0 }
    END { print answer }
  '
}

whitelist_add() {
  local name="$1"
  local since
  local answer
  local waited=0

  # two seconds back, for clock skew between this machine and the node
  since="$(date -u -d '-2 seconds' +%Y-%m-%dT%H:%M:%SZ)"
  console "/player ${name} whitelist on"

  while true; do
    answer="$(answer_since "${name}" "${since}")"
    case "${answer}" in
      *"Player is now whitelisted"*)
        echo "-> ${name}: ${answer#*\[Server Notification\] }"
        return 0
        ;;
      "")
        if (( waited >= ANSWER_TIMEOUT_SECONDS )); then
          echo "-> ${name}: no answer from the server after ${ANSWER_TIMEOUT_SECONDS}s, check: ${KUBECTL} logs ${DEPLOYMENT} --tail=20" >&2
          return 1
        fi
        sleep 1
        waited=$((waited + 1))
        ;;
      *)
        echo "-> ${name}: ${answer#*\[Server Notification\] }" >&2
        return 1
        ;;
    esac
  done
}

require_cmd "${KUBECTL}"

for name in "$@"; do
  if [[ ! "${name}" =~ ^[^[:space:]/]+$ ]]; then
    echo "invalid username '${name}': no spaces or slashes" >&2
    exit 1
  fi
done

failed=()
for name in "$@"; do
  whitelist_add "${name}" || failed+=("${name}")
done

echo
echo "-> whitelist on the volume:"
kube exec "${DEPLOYMENT}" -- cat /data/Playerdata/playerswhitelisted.json \
  | sed -n 's/.*"PlayerName" *: *"\([^"]*\)".*/   \1/p'

case "${#failed[@]}" in
  0)
    ;;
  *)
    echo
    echo "-> not whitelisted: ${failed[*]}" >&2
    exit 1
    ;;
esac
