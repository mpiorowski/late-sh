#!/usr/bin/env bash
#
# Dump all messages from one or more public topic chat rooms (by slug) into
# plain-text files, one file per room. Read-only; never prints DB secrets.
#
# Usage:
#   scripts/dump_chat_room.sh                      # defaults: bugs suggestions
#   scripts/dump_chat_room.sh bugs suggestions feedback
#
# Output:
#   chat_dumps/<slug>.txt   (override dir with LATE_DUMP_DIR)
#
# Optional env (same conventions as scripts/connect_db.sh):
#   KUBECTL=kubectl  KUBE_CONTEXT=<ctx>  KUBE_NAMESPACE=default
#   LATE_DB_KUBE_SERVICE=postgres-rw  LATE_DB_KUBE_SECRET=postgres-app
#   LATE_DB_KUBE_POD=<pod>  LATE_DB_LOCAL_PORT=<port>  LATE_DUMP_DIR=chat_dumps

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
OUT_DIR="${LATE_DUMP_DIR:-chat_dumps}"

SLUGS=("$@")
if [[ ${#SLUGS[@]} -eq 0 ]]; then
  SLUGS=(bugs suggestions)
fi

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

mkdir -p "${OUT_DIR}"
export PGPASSFILE="${PGPASSFILE_PATH}" PGSSLMODE=disable
RUN_PSQL=("${PSQL}" -h "${LOCAL_HOST}" -p "${LOCAL_PORT}" -U "${DB_USER}" -d "${DB_NAME}" \
  -v ON_ERROR_STOP=1 -P pager=off --set=default_transaction_read_only=on)

# Per-message block:
#   [YYYY-MM-DD HH:MI:SS UTC] username [pinned] (reply to other: "snippet")
#   <body>
#   ----------------------------------------------------------------------
MSG_SQL_PATH="${TMP_DIR}/dump.sql"
cat >"${MSG_SQL_PATH}" <<'SQL'
select
  E'\n' ||
  '[' || to_char(m.created at time zone 'UTC', 'YYYY-MM-DD HH24:MI:SS') || ' UTC] ' ||
  coalesce(u.username, '<deleted>') ||
  case when m.pinned then ' [pinned]' else '' end ||
  case when m.reply_to_message_id is not null then
    ' (reply to ' || coalesce(ru.username, '<deleted>') || ': "' ||
    left(regexp_replace(coalesce(rm.body, '<missing>'), '\s+', ' ', 'g'), 60) || '")'
  else '' end ||
  E'\n' || m.body || E'\n' ||
  '----------------------------------------------------------------------'
from chat_messages m
join chat_rooms r on r.id = m.room_id
join users u on u.id = m.user_id
left join chat_messages rm on rm.id = m.reply_to_message_id
left join users ru on ru.id = rm.user_id
where r.slug = :'slug' and r.kind = 'topic'
order by m.created, m.id;
SQL

for slug in "${SLUGS[@]}"; do
  count="$("${RUN_PSQL[@]}" -tA -c \
    "select count(*) from chat_messages m join chat_rooms r on r.id=m.room_id where r.slug='${slug//\'/\'\'}' and r.kind='topic';")"
  if [[ -z "${count}" || "${count}" == "0" ]]; then
    echo "!! no topic room '${slug}' (or it has 0 messages); skipping" >&2
    continue
  fi
  out="${OUT_DIR}/${slug}.txt"
  {
    echo "=== #${slug} — ${count} messages — dumped $(date -u +%Y-%m-%dT%H:%M:%SZ) ==="
  } >"${out}"
  "${RUN_PSQL[@]}" -tA -v slug="${slug}" -f "${MSG_SQL_PATH}" >>"${out}"
  echo "-> wrote ${count} messages to ${out}"
done

echo "done."
