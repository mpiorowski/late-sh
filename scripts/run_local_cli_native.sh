#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

env_or_default() {
  local key="$1"
  local fallback="$2"
  local env_file="${ROOT_DIR}/.env"

  if [[ -f "${env_file}" ]]; then
    local value
    value="$(grep -E "^${key}=" "${env_file}" | tail -n 1 | cut -d= -f2- || true)"
    if [[ -n "${value}" ]]; then
      value="$(printf '%s' "${value}" | sed 's/^[[:space:]]*//; s/[[:space:]]*$//')"
      printf '%s\n' "${value}"
      return
    fi
  fi

  printf '%s\n' "${fallback}"
}

SSH_PORT="${LATE_LOCAL_SSH_PORT:-$(env_or_default LATE_SSH_PORT 2222)}"
API_PORT="$(env_or_default LATE_API_PORT 4000)"
WEB_PORT="$(env_or_default LATE_WEB_PORT 3000)"
API_BASE_URL="${LATE_LOCAL_API_BASE_URL:-http://localhost:${API_PORT}}"
AUDIO_BASE_URL="${LATE_LOCAL_AUDIO_BASE_URL:-http://localhost:${WEB_PORT}/stream}"
SSH_TARGET="${LATE_LOCAL_SSH_TARGET:-localhost}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required" >&2
  exit 1
fi

cd "${ROOT_DIR}"

cmd=(
  cargo run -p late-cli --bin late --
  --ssh-mode native
  --ssh-target "${SSH_TARGET}"
  --ssh-port "${SSH_PORT}"
  --api-base-url "${API_BASE_URL}"
  --audio-base-url "${AUDIO_BASE_URL}"
)

if [[ -n "${LATE_LOCAL_SSH_USER:-}" ]]; then
  cmd+=(--ssh-user "${LATE_LOCAL_SSH_USER}")
fi

exec "${cmd[@]}" "$@"
