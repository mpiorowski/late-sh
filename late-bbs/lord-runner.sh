#!/usr/bin/env bash
set -euo pipefail

LORD_HOME="${LORD_HOME:-/bbs/doors/lord}"

if [[ ! -d "${LORD_HOME}" ]]; then
  echo "LORD directory is missing: ${LORD_HOME}" >&2
  exit 1
fi

if [[ ! -f "${LORD_HOME}/LORD.EXE" && ! -f "${LORD_HOME}/lord.exe" ]]; then
  echo "Install the registered LORD BBS door files into ${LORD_HOME} before enabling this door." >&2
  exit 1
fi

cd "${LORD_HOME}"

# Synchronet external-program setup should pass the needed drop-file/node
# arguments. Keep the wrapper thin until V1 testing confirms the exact LORD
# command-line expected by the registered package.
exec dosemu -quiet -K "${LORD_HOME}" "$@"
