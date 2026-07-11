#!/usr/bin/env bash

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: $0 <username> [late args...]" >&2
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
username="$1"
shift

if [[ ! "${username}" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo "username may only contain letters, numbers, dot, underscore, and dash" >&2
  exit 1
fi

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
KEY_DIR="${LATE_LOCAL_USER_KEY_DIR:-/tmp/late-sh-users}"
KEY_PATH="${KEY_DIR}/${username}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required" >&2
  exit 1
fi

if ! command -v ssh-keygen >/dev/null 2>&1; then
  echo "ssh-keygen is required" >&2
  exit 1
fi

mkdir -p "${KEY_DIR}"

if [[ ! -f "${KEY_PATH}" ]]; then
  ssh-keygen -t ed25519 -f "${KEY_PATH}" -N "" -C "late-local-${username}" >/dev/null
  echo "created SSH key ${KEY_PATH}" >&2
fi

cd "${ROOT_DIR}"

# Build the Linux webview helper so the debug `late` finds its sibling
# `late-webview` in target/debug (embedded YouTube playback).
if [[ "$(uname -s)" == "Linux" ]]; then
  cargo build -p late-webview --bin late-webview
  export LATE_WEBVIEW_BIN="${LATE_WEBVIEW_BIN:-${CARGO_TARGET_DIR:-${ROOT_DIR}/target}/debug/late-webview}"
fi

exec cargo run -p late-cli --bin late -- \
  --ssh-mode native \
  --ssh-target "${SSH_TARGET}" \
  --ssh-port "${SSH_PORT}" \
  --ssh-user "${username}" \
  --key "${KEY_PATH}" \
  --api-base-url "${API_BASE_URL}" \
  --audio-base-url "${AUDIO_BASE_URL}" \
  "$@"
