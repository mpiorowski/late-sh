#!/usr/bin/env bash
# Populate today's local Docker Compose newspaper without running the press.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/seed_paper_test_data.sh [PARAGRAPHS]

Replace today's Reading and Outside sections with a fake newspaper and enable
the paper in the local Compose database. PARAGRAPHS is 1-1000 (default: 100).
Open /paper in the TUI; close and reopen it after each seed. No restart needed.

Run make start first so the database migrations have been applied.
Optional environment: LATE_DB_USER, LATE_DB_NAME (both default to postgres).
The usual COMPOSE_PROJECT_NAME and COMPOSE_FILE overrides are supported.
USAGE
}

if [[ ${1:-} == -h || ${1:-} == --help ]]; then
  usage
  exit 0
fi

PARAGRAPHS=${1-100}
if (( $# > 1 )) || [[ ! $PARAGRAPHS =~ ^[1-9][0-9]{0,3}$ ]] || (( PARAGRAPHS > 1000 )); then
  usage >&2
  exit 2
fi

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd -- "$ROOT_DIR"

if ! command -v docker >/dev/null 2>&1 || ! docker compose version >/dev/null 2>&1; then
  echo 'docker compose is required' >&2
  exit 1
fi

echo '-> ensuring local postgres is ready'
docker compose up -d --wait postgres >/dev/null

echo "-> seeding today's paper with $PARAGRAPHS sample paragraphs"
docker compose exec -T postgres psql -X \
  -U "${LATE_DB_USER:-postgres}" -d "${LATE_DB_NAME:-postgres}" \
  -v ON_ERROR_STOP=1 -v "paper_paragraphs=$PARAGRAPHS" \
  <"$ROOT_DIR/scripts/seed_paper_test_data.sql"

echo 'Ready: open /paper in the TUI (close and reopen if it is already visible).'
