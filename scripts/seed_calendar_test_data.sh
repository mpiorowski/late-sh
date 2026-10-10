#!/usr/bin/env bash
# Repeatable fixture in the local Compose database only.
set -euo pipefail
ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$ROOT_DIR"
if [[ ${1:-} == --help || ${1:-} == -h ]]; then
  echo 'Seed cal_user, cal_public, cal_private, cal_mod, cal_admin and representative calendars.'
  echo 'Run the migrated local stack first. Keys: tmp/calendar-seed-keys/cal_ACCOUNT.'
  exit 0
fi
if (( $# != 0 )); then echo 'No arguments expected' >&2; exit 2; fi
PSQL=(docker compose exec -T postgres psql -X -U "${LATE_DB_USER:-postgres}" -d "${LATE_DB_NAME:-postgres}" -v ON_ERROR_STOP=1)
docker compose up -d --wait postgres >/dev/null
if [[ $("${PSQL[@]}" -Atc "SELECT to_regclass('calendar_events') IS NOT NULL") != t ]]; then
  echo 'Calendar migration is missing. Start the current SSH build first.' >&2; exit 3
fi
KEY_DIR="$ROOT_DIR/tmp/calendar-seed-keys"
mkdir -p "$KEY_DIR"
chmod 700 "$KEY_DIR"
KEY_CSV=$(mktemp)
trap 'rm -f "$KEY_CSV"' EXIT
for account in user public private mod admin; do
  key="$KEY_DIR/cal_$account"
  if [[ ! -f $key ]]; then ssh-keygen -q -t ed25519 -N '' -C "local-calendar-seed:$account" -f "$key"; fi
  if [[ ! -f $key.pub ]]; then ssh-keygen -y -f "$key" > "$key.pub"; fi
  fingerprint=$(ssh-keygen -E sha256 -lf "$key.pub" | awk '{print $2}')
  printf '%s,%s\n' "$account" "$fingerprint" >> "$KEY_CSV"
done
{
  printf '%s\n' 'CREATE TEMP TABLE seed_calendar_keys(account text PRIMARY KEY,fingerprint text NOT NULL UNIQUE);' 'COPY seed_calendar_keys FROM STDIN WITH (FORMAT csv);'
  cat "$KEY_CSV"
  printf '\\.%s\n' ''
  cat "$ROOT_DIR/scripts/seed_calendar_test_data.sql"
} | "${PSQL[@]}"
echo 'Ready: ssh -o IdentitiesOnly=yes -i tmp/calendar-seed-keys/cal_user -p 2222 localhost'
echo 'Press 7; s selects server/personal/public calendars. Rerun to reset only fixture events.'
