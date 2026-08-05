#!/usr/bin/env bash
#
# Populate the local Docker Compose database with synthetic leaderboard data.
#
# Usage:
#   scripts/seed_leaderboard_test_data.sh
#   scripts/seed_leaderboard_test_data.sh USERNAME
#
# Optional env:
#   LATE_DB_USER=postgres
#   LATE_DB_NAME=postgres

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if (( $# > 1 )); then
  echo "usage: $0 [USERNAME]" >&2
  exit 2
fi

if (( $# == 1 )) && [[ -z $1 ]]; then
  echo "username must not be empty" >&2
  exit 2
fi

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

PSQL_ARGS=(
  -U "${LATE_DB_USER:-postgres}"
  -d "${LATE_DB_NAME:-postgres}"
  -v ON_ERROR_STOP=1
)

if (( $# == 1 )); then
  PSQL_ARGS+=(-v "leaderboard_username=$1")

  USERNAME_EXISTS="$(
    "${COMPOSE[@]}" exec -T postgres psql \
      "${PSQL_ARGS[@]}" -At <<'SQL'
SELECT EXISTS (
  SELECT 1
  FROM users
  WHERE username NOT IN ('system', 'bot', 'bartender')
    AND fingerprint NOT LIKE 'seed:leaderboard:%'
    AND LOWER(username) = LOWER(:'leaderboard_username')
);
SQL
  )"
  if [[ $USERNAME_EXISTS != t ]]; then
    echo "requested leaderboard username was not found: $1" >&2
    exit 3
  fi

  echo "-> replacing synthetic leaderboard activity; enriching user: $1"
else
  echo "-> replacing synthetic leaderboard activity; enriching most recently active user"
fi

"${COMPOSE[@]}" exec -T postgres psql \
  "${PSQL_ARGS[@]}" \
  <"${SCRIPT_DIR}/seed_leaderboard_test_data.sql"
