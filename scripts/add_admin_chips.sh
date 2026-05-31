#!/usr/bin/env bash
#
# Add Late Chips to users in the local Docker Compose database.
#
# Usage:
#   scripts/add_admin_chips.sh <amount>
#   scripts/add_admin_chips.sh <amount> --username <name>
#   scripts/add_admin_chips.sh <amount> --user-id <uuid>
#
# Default target is every user row. Pass --username or --user-id to narrow it.

set -euo pipefail

usage() {
  sed -n '2,12p' "$0" >&2
}

if [[ $# -lt 1 ]]; then
  usage
  exit 1
fi

AMOUNT="$1"
shift

if ! [[ "${AMOUNT}" =~ ^[1-9][0-9]*$ ]]; then
  echo "amount must be a positive integer" >&2
  exit 1
fi

MODE="all_users"
USERNAME=""
USER_ID=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --username)
      [[ $# -ge 2 ]] || { echo "--username requires a value" >&2; exit 1; }
      MODE="username"
      USERNAME="$2"
      shift 2
      ;;
    --user-id)
      [[ $# -ge 2 ]] || { echo "--user-id requires a value" >&2; exit 1; }
      MODE="user_id"
      USER_ID="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ "${MODE}" == "username" && -z "${USERNAME}" ]]; then
  echo "--username cannot be empty" >&2
  exit 1
fi

if [[ "${MODE}" == "user_id" && -z "${USER_ID}" ]]; then
  echo "--user-id cannot be empty" >&2
  exit 1
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

echo "-> adding ${AMOUNT} chips (${MODE})"
"${COMPOSE[@]}" exec -T postgres psql \
  -U "${LATE_DB_USER:-postgres}" \
  -d "${LATE_DB_NAME:-postgres}" \
  -v ON_ERROR_STOP=1 \
  -v amount="${AMOUNT}" \
  -v mode="${MODE}" \
  -v username="${USERNAME}" \
  -v user_id="${USER_ID}" <<'SQL'
WITH target_users AS (
    SELECT id, username
    FROM users
    WHERE
        (:'mode' = 'all_users')
        OR (:'mode' = 'username' AND lower(username) = lower(:'username'))
        OR (:'mode' = 'user_id' AND id = NULLIF(:'user_id', '')::uuid)
),
upserted AS (
    INSERT INTO user_chips (user_id, balance)
    SELECT id, :amount::bigint
    FROM target_users
    ON CONFLICT (user_id) DO UPDATE SET
        balance = user_chips.balance + EXCLUDED.balance,
        updated = current_timestamp
    RETURNING user_id, balance
),
ledger AS (
    INSERT INTO chip_ledger (user_id, delta, reason, source_kind, source_ref)
    SELECT user_id, :amount::bigint, 'admin_chip_grant', 'script', 'scripts/add_admin_chips.sh'
    FROM upserted
    RETURNING 1
),
notified AS (
    SELECT pg_notify('chip_user_changed', user_id::text)
    FROM upserted
)
SELECT
    u.username,
    u.id,
    :amount::bigint AS added,
    upserted.balance AS new_balance
FROM upserted
JOIN users u ON u.id = upserted.user_id
ORDER BY u.username;
SQL
