#!/usr/bin/env bash
#
# Add players to the production Minecraft whitelist, effective immediately.
# Checks each name against Mojang first (a typo would whitelist nobody), then
# runs `whitelist add` over RCON inside the pod. No restart; whitelist.json on
# the world volume is rewritten on the spot, so the entry survives restarts.
#
# Usage:
#   scripts/minecraft_whitelist_add.sh RenderingUser
#   scripts/minecraft_whitelist_add.sh name1 name2 name3
#
# The GitHub variable MINECRAFT_WHITELIST only seeds the list at boot, so it
# is not touched here. The script prints the exact gh command that adds any
# names it is missing, for a volume recreated from scratch.
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
DEPLOYMENT="deploy/minecraft"

KUBECTL_ARGS=()
if [[ -n "${KUBE_CONTEXT:-}" ]]; then
  KUBECTL_ARGS+=(--context "${KUBE_CONTEXT}")
fi

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "$1 is required" >&2; exit 1; }
}

rcon() {
  "${KUBECTL}" "${KUBECTL_ARGS[@]}" exec -n "${KUBE_NAMESPACE}" "${DEPLOYMENT}" -- rcon-cli "$@"
}

# Prints the canonical Java Edition name for $1, or fails with a reason.
mojang_name() {
  local name="$1"
  local body
  local code

  if [[ ! "${name}" =~ ^[A-Za-z0-9_]{3,16}$ ]]; then
    echo "invalid username '${name}': 3-16 letters, digits or underscores" >&2
    return 1
  fi

  body="$(mktemp)"
  code="$(curl -sS -o "${body}" -w '%{http_code}' "https://api.mojang.com/users/profiles/minecraft/${name}")"

  case "${code}" in
    200)
      sed -n 's/.*"name" *: *"\([A-Za-z0-9_]*\)".*/\1/p' "${body}" | head -n1
      rm -f "${body}"
      ;;
    204 | 404)
      rm -f "${body}"
      echo "no Java Edition account named '${name}'" >&2
      return 1
      ;;
    *)
      echo "mojang lookup for '${name}' failed with http ${code}: $(cat "${body}")" >&2
      rm -f "${body}"
      return 1
      ;;
  esac
}

require_cmd "${KUBECTL}"
require_cmd curl
require_cmd gh

names=()
for arg in "$@"; do
  canonical="$(mojang_name "${arg}")"
  echo "-> ${arg}: Java Edition account ${canonical}"
  names+=("${canonical}")
done

for name in "${names[@]}"; do
  echo "-> $(rcon whitelist add "${name}")"
done

echo "-> $(rcon whitelist list)"

seed="$(gh variable get MINECRAFT_WHITELIST --env production)"
merged="${seed}"
missing=()
for name in "${names[@]}"; do
  if [[ ",${seed,,}," != *",${name,,},"* ]]; then
    missing+=("${name}")
    merged="${merged:+${merged},}${name}"
  fi
done

echo
case "${#missing[@]}" in
  0)
    echo "-> MINECRAFT_WHITELIST already seeds every name, nothing to update"
    ;;
  *)
    echo "-> not in MINECRAFT_WHITELIST yet: ${missing[*]}. To seed them at boot too:"
    echo "  gh variable set MINECRAFT_WHITELIST --env production --body \"${merged}\""
    ;;
esac
