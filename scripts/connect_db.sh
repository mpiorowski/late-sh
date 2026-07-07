#!/usr/bin/env bash
#
# Connect local pgcli to the production CloudNativePG database through kubectl.
#
# Usage:
#   scripts/connect_db.sh
#   scripts/connect_db.sh -- <extra pgcli args>
#
# Optional env:
#   KUBECTL=kubectl
#   PGCLI=pgcli
#   KUBE_CONTEXT=<kubectl context>
#   KUBE_NAMESPACE=default
#   LATE_DB_KUBE_SERVICE=postgres-rw
#   LATE_DB_KUBE_SECRET=postgres-app
#   LATE_DB_KUBE_POD=<postgres pod name>
#   LATE_DB_LOCAL_PORT=15432

set -euo pipefail

usage() {
  sed -n '2,17p' "$0" >&2
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "${1:-}" == "--" ]]; then
  shift
fi

KUBECTL="${KUBECTL:-kubectl}"
PGCLI="${PGCLI:-pgcli}"
KUBE_NAMESPACE="${KUBE_NAMESPACE:-default}"
DB_SERVICE="${LATE_DB_KUBE_SERVICE:-postgres-rw}"
DB_SECRET="${LATE_DB_KUBE_SECRET:-postgres-app}"
DB_REMOTE_PORT="${LATE_DB_KUBE_PORT:-5432}"
DB_POD="${LATE_DB_KUBE_POD:-}"
LOCAL_HOST="127.0.0.1"
LOCAL_PORT="${LATE_DB_LOCAL_PORT:-}"

KUBECTL_ARGS=()
if [[ -n "${KUBE_CONTEXT:-}" ]]; then
  KUBECTL_ARGS+=(--context "${KUBE_CONTEXT}")
fi

require_cmd() {
  local cmd="$1"
  if ! command -v "${cmd}" >/dev/null 2>&1; then
    echo "${cmd} is required" >&2
    exit 1
  fi
}

secret_value() {
  local key="$1"
  local encoded

  encoded="$(
    "${KUBECTL}" "${KUBECTL_ARGS[@]}" get secret -n "${KUBE_NAMESPACE}" "${DB_SECRET}" \
      -o "jsonpath={.data.${key}}"
  )"

  if [[ -z "${encoded}" ]]; then
    echo "secret ${DB_SECRET} is missing key ${key}" >&2
    exit 1
  fi

  printf '%s' "${encoded}" | decode_base64
}

service_pod() {
  local pod

  pod="$(
    "${KUBECTL}" "${KUBECTL_ARGS[@]}" get endpoints -n "${KUBE_NAMESPACE}" "${DB_SERVICE}" \
      -o 'jsonpath={.subsets[0].addresses[0].targetRef.name}'
  )"

  if [[ -z "${pod}" ]]; then
    echo "service ${DB_SERVICE} has no ready pod endpoint; set LATE_DB_KUBE_POD to override" >&2
    exit 1
  fi

  printf '%s' "${pod}"
}

decode_base64() {
  if base64 --decode </dev/null >/dev/null 2>&1; then
    base64 --decode
  else
    base64 -D
  fi
}

pgpass_escape() {
  local value="$1"
  value="${value//\\/\\\\}"
  value="${value//:/\\:}"
  printf '%s' "${value}"
}

port_is_open() {
  local host="$1"
  local port="$2"
  (exec 3<>"/dev/tcp/${host}/${port}") 2>/dev/null
}

pick_local_port() {
  if [[ -n "${LOCAL_PORT}" ]]; then
    printf '%s' "${LOCAL_PORT}"
    return
  fi

  for port in $(seq 15432 15462); do
    if ! port_is_open "${LOCAL_HOST}" "${port}"; then
      printf '%s' "${port}"
      return
    fi
  done

  echo "no free local port found in 15432-15462; set LATE_DB_LOCAL_PORT" >&2
  exit 1
}

cleanup() {
  if [[ -n "${PF_PID:-}" ]]; then
    kill "${PF_PID}" 2>/dev/null || true
    wait "${PF_PID}" 2>/dev/null || true
  fi

  if [[ -n "${TMP_DIR:-}" ]]; then
    rm -rf "${TMP_DIR}"
  fi
}

require_cmd "${KUBECTL}"
require_cmd "${PGCLI}"
require_cmd base64

LOCAL_PORT="$(pick_local_port)"
TMP_DIR="$(mktemp -d)"
PGPASSFILE_PATH="${TMP_DIR}/pgpass"
PF_LOG="${TMP_DIR}/kubectl-port-forward.log"
trap cleanup EXIT INT TERM

echo "-> reading database connection metadata from Kubernetes secret ${DB_SECRET}"
DB_USER="$(secret_value user)"
DB_PASSWORD="$(secret_value password)"
DB_NAME="$(secret_value dbname)"
if [[ -z "${DB_POD}" ]]; then
  DB_POD="$(service_pod)"
fi

chmod 700 "${TMP_DIR}"
printf '%s:%s:%s:%s:%s\n' \
  "${LOCAL_HOST}" \
  "${LOCAL_PORT}" \
  "$(pgpass_escape "${DB_NAME}")" \
  "$(pgpass_escape "${DB_USER}")" \
  "$(pgpass_escape "${DB_PASSWORD}")" \
  >"${PGPASSFILE_PATH}"
chmod 600 "${PGPASSFILE_PATH}"
unset DB_PASSWORD

echo "-> port-forwarding pod/${DB_POD} (${DB_SERVICE}) ${LOCAL_HOST}:${LOCAL_PORT} -> ${DB_REMOTE_PORT}"
"${KUBECTL}" "${KUBECTL_ARGS[@]}" port-forward -n "${KUBE_NAMESPACE}" \
  "pod/${DB_POD}" "${LOCAL_PORT}:${DB_REMOTE_PORT}" >"${PF_LOG}" 2>&1 &
PF_PID=$!

ready=0
for _ in $(seq 1 100); do
  if ! kill -0 "${PF_PID}" 2>/dev/null; then
    echo "kubectl port-forward exited early:" >&2
    sed -n '1,120p' "${PF_LOG}" >&2
    exit 1
  fi

  if grep -q '^Forwarding from ' "${PF_LOG}"; then
    ready=1
    break
  fi

  sleep 0.1
done

if [[ "${ready}" != "1" ]]; then
  echo "timed out waiting for kubectl port-forward" >&2
  sed -n '1,120p' "${PF_LOG}" >&2
  exit 1
fi

echo "-> opening pgcli as ${DB_USER}@${DB_SERVICE}/${DB_NAME}"
PGPASSFILE="${PGPASSFILE_PATH}" PGSSLMODE=disable "${PGCLI}" \
  -h "${LOCAL_HOST}" \
  -p "${LOCAL_PORT}" \
  -U "${DB_USER}" \
  -d "${DB_NAME}" \
  "$@"
