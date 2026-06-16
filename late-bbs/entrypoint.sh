#!/usr/bin/env bash
set -euo pipefail

SBBS_HOME="${SBBS_HOME:-/bbs/sbbs}"
LORD_HOME="${LORD_HOME:-/bbs/doors/lord}"
SBBSCTRL="${SBBSCTRL:-${SBBS_HOME}/ctrl}"
SBBSEXEC="${SBBSEXEC:-${SBBS_HOME}/exec}"

mkdir -p /bbs/import /bbs/doors /bbs/dosemu /bbs/backups

if [[ ! -d "${SBBS_HOME}/ctrl" ]]; then
  echo "Initializing Synchronet data directory at ${SBBS_HOME}"
  mkdir -p "${SBBS_HOME}"
  cp -a /opt/sbbs/. "${SBBS_HOME}/"
fi

if [[ ! -e /sbbs ]]; then
  ln -s "${SBBS_HOME}" /sbbs
fi

mkdir -p "${LORD_HOME}"

if [[ ! -e "${SBBS_HOME}/exec/lord-runner" ]]; then
  ln -s /usr/local/bin/lord-runner "${SBBS_HOME}/exec/lord-runner"
fi

if [[ -f "${SBBSCTRL}/sbbs.ini" ]] && ! grep -q "late.sh LORD BBS container defaults" "${SBBSCTRL}/sbbs.ini"; then
  cat >> "${SBBSCTRL}/sbbs.ini" <<'EOF'

; late.sh LORD BBS container defaults
[UNIX]
User=sbbs
Group=sbbs
EOF
fi

chown -R sbbs:sbbs /bbs

if [[ ! -x "${SBBSEXEC}/sbbs" ]]; then
  echo "Synchronet executable missing: ${SBBSEXEC}/sbbs" >&2
  exit 1
fi

echo "Starting Synchronet with SBBSCTRL=${SBBSCTRL}"
exec env SBBSCTRL="${SBBSCTRL}" SBBSEXEC="${SBBSEXEC}" "${SBBSEXEC}/sbbs"
