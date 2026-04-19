#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required" >&2
  exit 1
fi

cd "${ROOT_DIR}"

exec env \
  RUST_LOG="${RUST_LOG:-late=debug,late_core=debug}" \
  cargo run -p late-tools --bin late -- \
    --ssh-target "${LATE_PROD_SSH_TARGET:-late.sh}" \
    --api-base-url "${LATE_PROD_API_BASE_URL:-https://api.late.sh}" \
    --audio-base-url "${LATE_PROD_AUDIO_BASE_URL:-https://audio.late.sh}" \
    --verbose \
    "$@"
