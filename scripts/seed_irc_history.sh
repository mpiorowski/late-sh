#!/usr/bin/env bash
# Synthetic conversation history in the local Compose database.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/seed_irc_history.sh USERNAME

Seed history for an existing local account: 400 messages in #history-lab,
80 in ##history-lab, 80 in each of two DMs, and 40 in #lounge.
Numbered messages include replies, Unicode, multiline and long bodies, and
equal timestamps. Rerunning restores only these fixture messages. Existing
messages, tokens, account preferences and read markers are preserved.

Run the current app first to apply migrations and create your account.
Reconnect Halloy after seeding, then /join #history-lab and /join ##history-lab.
DM peers: history_ada and history^lin (the latter is history.lin in the TUI).
This inserts stored history; it does not broadcast live chat events.

Optional environment: LATE_DB_USER, LATE_DB_NAME (default: postgres).
The usual COMPOSE_PROJECT_NAME and COMPOSE_FILE overrides are supported.
USAGE
}

if [[ ${1:-} == -h || ${1:-} == --help ]]; then
  usage
  exit 0
fi
if (( $# != 1 )) || [[ -z $1 ]]; then
  usage >&2
  exit 2
fi

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd -- "$ROOT_DIR"
if ! command -v docker >/dev/null 2>&1 || ! docker compose version >/dev/null 2>&1; then
  echo 'docker compose is required' >&2
  exit 1
fi

docker compose up -d --wait postgres >/dev/null
docker compose exec -T postgres psql -X \
  -U "${LATE_DB_USER:-postgres}" -d "${LATE_DB_NAME:-postgres}" \
  -v ON_ERROR_STOP=1 -v "history_username=$1" \
  <"$ROOT_DIR/scripts/seed_irc_history.sql"

echo 'Ready: reconnect Halloy, then /join #history-lab and /join ##history-lab.'
echo 'Scroll upward to fetch older pages. DM peers: history_ada and history^lin.'
