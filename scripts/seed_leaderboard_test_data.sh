#!/usr/bin/env bash
#
# Populate the local Docker Compose database with synthetic leaderboard data.
#
# Usage:
#   scripts/seed_leaderboard_test_data.sh
#
# Optional env:
#   LATE_DB_USER=postgres
#   LATE_DB_NAME=postgres

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if ! command -v docker >/dev/null 2>&1; then
  echo "docker is required" >&2
  exit 1
fi

COMPOSE=(docker compose)
if ! "${COMPOSE[@]}" version >/dev/null 2>&1; then
  if command -v docker-compose >/dev/null 2>&1; then
    COMPOSE=(docker-compose)
  else
    echo "docker compose is required" >&2
    exit 1
  fi
fi

echo "-> ensuring local postgres is running"
"${COMPOSE[@]}" up -d postgres >/dev/null

echo "-> replacing synthetic leaderboard activity"
"${COMPOSE[@]}" exec -T postgres psql \
  -U "${LATE_DB_USER:-postgres}" \
  -d "${LATE_DB_NAME:-postgres}" \
  -v ON_ERROR_STOP=1 \
  <"${SCRIPT_DIR}/seed_leaderboard_test_data.sql"
