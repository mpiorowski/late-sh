#!/usr/bin/env bash
#
# Delete chat messages by id, and nothing else. Pair it with
# scripts/dump_chat_room.sh, whose message headers end with `id=<uuid>`: dump a
# room, decide which entries are handled, hand their ids to this script.
#
# It only ever runs `delete from chat_messages where id = any(<the ids>)`.
# There is no slug, room, user, or date filter on purpose: the id list is the
# whole contract, so a typo can never widen the blast radius.
#
# Usage:
#   scripts/delete_chat_messages.sh <uuid> [<uuid> ...]
#   scripts/delete_chat_messages.sh -f ids.txt
#   grep -o 'id=[0-9a-f-]\{36\}' feedback/suggestions.txt | cut -d= -f2 \
#     | scripts/delete_chat_messages.sh -
#
# Flags:
#   -f FILE   read ids from FILE (whitespace separated; # starts a comment)
#   -         read ids from stdin (same format)
#   -n        dry run: print the preview, delete nothing
#   -y        skip the confirmation prompt (for scripted cleanups)
#
# The preview shows room, timestamp, author, and a body snippet for every id,
# flags ids that no longer exist, and warns when other messages reply to one of
# the targets (those replies survive with their reply target cleared, per the
# ON DELETE SET NULL on chat_messages.reply_to_message_id). Reactions,
# notifications, and translations for a deleted message cascade away.
#
# Sessions currently rendering a deleted message keep showing it until they
# refetch; this writes to the DB behind the running app, it does not broadcast.
#
# Optional env (same conventions as scripts/connect_db.sh):
#   KUBECTL=kubectl  KUBE_CONTEXT=<ctx>  KUBE_NAMESPACE=default
#   LATE_DB_KUBE_SERVICE=postgres-rw  LATE_DB_KUBE_SECRET=postgres-app
#   LATE_DB_KUBE_POD=<pod>  LATE_DB_LOCAL_PORT=<port>

set -euo pipefail

KUBECTL="${KUBECTL:-kubectl}"
PSQL="${PSQL:-psql}"
KUBE_NAMESPACE="${KUBE_NAMESPACE:-default}"
DB_SERVICE="${LATE_DB_KUBE_SERVICE:-postgres-rw}"
DB_SECRET="${LATE_DB_KUBE_SECRET:-postgres-app}"
DB_REMOTE_PORT="${LATE_DB_KUBE_PORT:-5432}"
DB_POD="${LATE_DB_KUBE_POD:-}"
LOCAL_HOST="127.0.0.1"
LOCAL_PORT="${LATE_DB_LOCAL_PORT:-}"

DRY_RUN=0
ASSUME_YES=0
RAW_IDS=()

usage() { sed -n '2,35p' "$0" >&2; }

read_ids_from() {
  local line word
  while IFS= read -r line || [[ -n "${line}" ]]; do
    line="${line%%#*}"
    for word in ${line}; do
      RAW_IDS+=("${word}")
    done
  done
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -h|--help) usage; exit 0 ;;
    -n|--dry-run) DRY_RUN=1; shift ;;
    -y|--yes) ASSUME_YES=1; shift ;;
    -f|--file)
      [[ -n "${2:-}" ]] || { echo "-f needs a file path" >&2; exit 1; }
      [[ -r "$2" ]] || { echo "cannot read ${2}" >&2; exit 1; }
      read_ids_from <"$2"
      shift 2 ;;
    -) read_ids_from; shift ;;
    -*) echo "unknown flag: $1" >&2; usage; exit 1 ;;
    *) RAW_IDS+=("$1"); shift ;;
  esac
done

if [[ ${#RAW_IDS[@]} -eq 0 ]]; then
  echo "no message ids given" >&2
  usage
  exit 1
fi

# Validate before anything else: every id must be a plain UUID, so the list can
# be inlined into SQL as a literal array with nothing left to escape.
UUID_RE='^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$'
IDS=()
for raw in "${RAW_IDS[@]}"; do
  id="${raw#id=}"
  id="${id,,}"
  [[ "${id}" =~ ${UUID_RE} ]] || { echo "not a uuid: ${raw}" >&2; exit 1; }
  duplicate=0
  for seen in ${IDS[@]+"${IDS[@]}"}; do
    [[ "${seen}" == "${id}" ]] && { duplicate=1; break; }
  done
  [[ "${duplicate}" == "1" ]] || IDS+=("${id}")
done

ID_ARRAY="{$(IFS=,; printf '%s' "${IDS[*]}")}"

KUBECTL_ARGS=()
if [[ -n "${KUBE_CONTEXT:-}" ]]; then
  KUBECTL_ARGS+=(--context "${KUBE_CONTEXT}")
fi

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || { echo "$1 is required" >&2; exit 1; }
}

decode_base64() {
  if base64 --decode </dev/null >/dev/null 2>&1; then base64 --decode; else base64 -D; fi
}

secret_value() {
  local key="$1" encoded
  encoded="$("${KUBECTL}" "${KUBECTL_ARGS[@]}" get secret -n "${KUBE_NAMESPACE}" "${DB_SECRET}" -o "jsonpath={.data.${key}}")"
  [[ -n "${encoded}" ]] || { echo "secret ${DB_SECRET} missing key ${key}" >&2; exit 1; }
  printf '%s' "${encoded}" | decode_base64
}

service_pod() {
  local pod
  pod="$("${KUBECTL}" "${KUBECTL_ARGS[@]}" get endpoints -n "${KUBE_NAMESPACE}" "${DB_SERVICE}" -o 'jsonpath={.subsets[0].addresses[0].targetRef.name}')"
  [[ -n "${pod}" ]] || { echo "service ${DB_SERVICE} has no ready pod; set LATE_DB_KUBE_POD" >&2; exit 1; }
  printf '%s' "${pod}"
}

pgpass_escape() {
  local v="$1"; v="${v//\\/\\\\}"; v="${v//:/\\:}"; printf '%s' "${v}"
}

port_is_open() { (exec 3<>"/dev/tcp/$1/$2") 2>/dev/null; }

pick_local_port() {
  if [[ -n "${LOCAL_PORT}" ]]; then printf '%s' "${LOCAL_PORT}"; return; fi
  for port in $(seq 15432 15462); do
    port_is_open "${LOCAL_HOST}" "${port}" || { printf '%s' "${port}"; return; }
  done
  echo "no free local port in 15432-15462; set LATE_DB_LOCAL_PORT" >&2; exit 1
}

cleanup() {
  [[ -n "${PF_PID:-}" ]] && { kill "${PF_PID}" 2>/dev/null || true; wait "${PF_PID}" 2>/dev/null || true; }
  [[ -n "${TMP_DIR:-}" ]] && rm -rf "${TMP_DIR}"
}

require_cmd "${KUBECTL}"
require_cmd "${PSQL}"
require_cmd base64

LOCAL_PORT="$(pick_local_port)"
TMP_DIR="$(mktemp -d)"; chmod 700 "${TMP_DIR}"
PGPASSFILE_PATH="${TMP_DIR}/pgpass"
PF_LOG="${TMP_DIR}/pf.log"
trap cleanup EXIT INT TERM

echo "-> reading connection metadata from secret ${DB_SECRET}"
DB_USER="$(secret_value user)"
DB_NAME="$(secret_value dbname)"
[[ -n "${DB_POD}" ]] || DB_POD="$(service_pod)"

# Password goes straight into the pgpass file; it is never echoed or passed on argv.
printf '%s:%s:%s:%s:%s\n' \
  "${LOCAL_HOST}" "${LOCAL_PORT}" \
  "$(pgpass_escape "${DB_NAME}")" "$(pgpass_escape "${DB_USER}")" \
  "$(pgpass_escape "$(secret_value password)")" \
  >"${PGPASSFILE_PATH}"
chmod 600 "${PGPASSFILE_PATH}"

echo "-> port-forwarding pod/${DB_POD} ${LOCAL_HOST}:${LOCAL_PORT} -> ${DB_REMOTE_PORT}"
"${KUBECTL}" "${KUBECTL_ARGS[@]}" port-forward -n "${KUBE_NAMESPACE}" \
  "pod/${DB_POD}" "${LOCAL_PORT}:${DB_REMOTE_PORT}" >"${PF_LOG}" 2>&1 &
PF_PID=$!

for _ in $(seq 1 100); do
  kill -0 "${PF_PID}" 2>/dev/null || { echo "port-forward exited early:" >&2; cat "${PF_LOG}" >&2; exit 1; }
  grep -q '^Forwarding from ' "${PF_LOG}" && break
  sleep 0.1
done
grep -q '^Forwarding from ' "${PF_LOG}" || { echo "timed out waiting for port-forward" >&2; cat "${PF_LOG}" >&2; exit 1; }

export PGPASSFILE="${PGPASSFILE_PATH}" PGSSLMODE=disable
PSQL_BASE=("${PSQL}" -h "${LOCAL_HOST}" -p "${LOCAL_PORT}" -U "${DB_USER}" -d "${DB_NAME}" \
  -v ON_ERROR_STOP=1 -P pager=off)
# The preview connection is pinned read-only; only the delete step below runs
# on a writable session. The pin has to travel as a libpq connection option:
# psql's --set defines psql variables, which the server never sees.
RUN_PSQL_RO=(env "PGOPTIONS=-c default_transaction_read_only=on" "${PSQL_BASE[@]}")
RUN_PSQL_RW=("${PSQL_BASE[@]}")

PREVIEW_SQL_PATH="${TMP_DIR}/preview.sql"
cat >"${PREVIEW_SQL_PATH}" <<'SQL'
select
  t.id::text || '  ' ||
  case when m.id is null then '<not found, already gone>'
  else
    coalesce('#' || r.slug, r.kind) || '  ' ||
    to_char(m.created at time zone 'UTC', 'YYYY-MM-DD HH24:MI:SS') || ' UTC  ' ||
    coalesce(u.username, '<deleted>') || '  "' ||
    left(regexp_replace(m.body, '\s+', ' ', 'g'), 100) || '"'
  end
from unnest((:'ids')::uuid[]) as t(id)
left join chat_messages m on m.id = t.id
left join chat_rooms r on r.id = m.room_id
left join users u on u.id = m.user_id
order by m.created nulls last, t.id;
SQL

echo
echo "=== ${#IDS[@]} message id(s) targeted ==="
"${RUN_PSQL_RO[@]}" -tA -v ids="${ID_ARRAY}" -f "${PREVIEW_SQL_PATH}"

# Every statement lives in a file rather than -c: the id list reaches the
# server as the psql variable :'ids', and -f is the interpolation path the
# dump scripts already rely on.
COUNT_SQL_PATH="${TMP_DIR}/counts.sql"
cat >"${COUNT_SQL_PATH}" <<'SQL'
select
  (select count(*) from chat_messages
    where id = any((:'ids')::uuid[])),
  (select count(*) from chat_messages
    where reply_to_message_id = any((:'ids')::uuid[])
      and id <> all((:'ids')::uuid[]));
SQL

counts="$("${RUN_PSQL_RO[@]}" -tA -F '|' -v ids="${ID_ARRAY}" -f "${COUNT_SQL_PATH}")"
found="${counts%%|*}"
orphaned="${counts#*|}"

echo
echo "-> ${found} of ${#IDS[@]} id(s) still exist"
if [[ "${orphaned}" != "0" ]]; then
  echo "!! ${orphaned} message(s) reply to a target; they survive with the reply target cleared" >&2
fi

if [[ "${found}" == "0" ]]; then
  echo "nothing to delete."
  exit 0
fi

if [[ "${DRY_RUN}" == "1" ]]; then
  echo "dry run: nothing deleted."
  exit 0
fi

if [[ "${ASSUME_YES}" != "1" ]]; then
  # Read from the terminal, not stdin: stdin may still be the piped id list.
  [[ -r /dev/tty ]] || { echo "no tty for confirmation; re-run with -y" >&2; exit 1; }
  printf 'delete these %s message(s) from %s? [y/N] ' "${found}" "${DB_NAME}"
  read -r answer </dev/tty
  case "${answer}" in
    y|Y|yes|YES) ;;
    *) echo "aborted."; exit 1 ;;
  esac
fi

DELETE_SQL_PATH="${TMP_DIR}/delete.sql"
cat >"${DELETE_SQL_PATH}" <<'SQL'
with gone as (
  delete from chat_messages
  where id = any((:'ids')::uuid[])
  returning id
)
select count(*) from gone;
SQL

deleted="$("${RUN_PSQL_RW[@]}" -tA -v ids="${ID_ARRAY}" -f "${DELETE_SQL_PATH}")"

echo "-> deleted ${deleted} message(s)"
echo "done."
