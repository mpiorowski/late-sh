#!/usr/bin/env bash
# Synthetic gallery fixtures in the local Docker Compose database.
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/seed_artboard_test_data.sh [SPLASH_PIECE]

Enable the gallery and seed 11 test accounts and 12 numbered ASCII pieces with
applause and content ratings. SPLASH_PIECE is 1-11 (default: 1); use 3, 5, 7, or 9
for an NSFW-rated splash. All drawings are harmless. Today's non-fixture
splash stays.
Rerunning restores the fixture pieces and their votes/marks, including test
changes made in the TUI. Existing accounts and other pieces are preserved.

Run make start first so the database migrations have been applied. Restart
service-ssh after seeding to refresh its cached splash, then reconnect.
Test SSH keys are retained in tmp/artboard-seed-keys (gitignored).
Optional environment: LATE_DB_USER, LATE_DB_NAME (both default to postgres).
The usual COMPOSE_PROJECT_NAME and COMPOSE_FILE overrides are supported.
USAGE
}

if [[ ${1:-} == -h || ${1:-} == --help ]]; then
  usage
  exit 0
fi

SPLASH_PIECE=${1-1}
if (( $# > 1 )) || [[ ! $SPLASH_PIECE =~ ^([1-9]|10|11)$ ]]; then
  usage >&2
  exit 2
fi

ROOT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd -- "$ROOT_DIR"

if ! command -v docker >/dev/null 2>&1 || ! docker compose version >/dev/null 2>&1; then
  echo 'docker compose is required' >&2
  exit 1
fi
if ! command -v ssh-keygen >/dev/null 2>&1; then
  echo 'ssh-keygen is required for the test account identities' >&2
  exit 1
fi

echo '-> ensuring local postgres is ready'
docker compose up -d --wait postgres >/dev/null
PSQL=(docker compose exec -T postgres psql -X
  -U "${LATE_DB_USER:-postgres}" -d "${LATE_DB_NAME:-postgres}"
  -v ON_ERROR_STOP=1)

SCHEMA_READY="$("${PSQL[@]}" -At <<'SQL'
SELECT to_regclass('artboard_piece_content_ratings') IS NOT NULL
   AND to_regclass('user_ssh_keys') IS NOT NULL;
SQL
)"
if [[ $SCHEMA_READY != t ]]; then
  echo 'Artboard rating migrations are missing. Run make start with the current code, then retry.' >&2
  exit 3
fi

# Stable logical identities follow the existing leaderboard seed convention;
# real SSH keys are registered separately so these accounts can also log in.
KEY_DIR="$ROOT_DIR/tmp/artboard-seed-keys"
mkdir -p -- "$KEY_DIR"
chmod 700 "$KEY_DIR"
KEY_CSV=$(mktemp)
trap 'rm -f -- "$KEY_CSV"' EXIT
PSQL_ARGS=(-v "splash_piece=$SPLASH_PIECE")
for ACCOUNT in artist1 artist2 artist3 voter1 voter2 voter3 voter4 mod1 mod2 admin1 admin2; do
  KEY_PATH="$KEY_DIR/art_$ACCOUNT"
  if [[ ! -f $KEY_PATH ]]; then
    ssh-keygen -q -t ed25519 -N '' -C "local-artboard-seed:$ACCOUNT" -f "$KEY_PATH"
  fi
  # Regenerate a missing public half from the retained private identity.
  if [[ ! -f $KEY_PATH.pub ]]; then
    ssh-keygen -y -f "$KEY_PATH" >"$KEY_PATH.pub"
  fi
  FINGERPRINT=$(ssh-keygen -E sha256 -lf "$KEY_PATH.pub" | awk '{print $2}')
  printf '%s,%s\n' "$ACCOUNT" "$FINGERPRINT" >>"$KEY_CSV"
done

echo "-> restoring art fixtures; selecting piece $SPLASH_PIECE for today's splash"
{
  # COPY data uses a separate stream; no generated strings are interpolated as SQL.
  printf '%s\n' 'CREATE TEMP TABLE seed_art_keys (account text PRIMARY KEY, fingerprint text NOT NULL UNIQUE);' \
    'COPY seed_art_keys (account, fingerprint) FROM STDIN WITH (FORMAT csv);'
  cat "$KEY_CSV"
  printf '\\.%s\n' ''
  cat "$ROOT_DIR/scripts/seed_artboard_test_data.sql"
} | "${PSQL[@]}" "${PSQL_ARGS[@]}"

cat <<'READY'
Ready: open Artboard (4), select Newest in the gallery rail, then press n on a
piece to vote. Log in as art_artist1/2/3 to exercise the owner flag:
  ssh -o IdentitiesOnly=yes -i tmp/artboard-seed-keys/art_artist1 -p 2222 localhost
Voters, moderators, and admins have matching keys in the same directory.
Refresh the splash cache with: docker compose restart service-ssh
READY
