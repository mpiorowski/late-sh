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

if [[ "$#" -eq 0 ]]; then
  dos_command="START.BAT 0"
else
  dos_command="$*"
fi

exec dosemu -quiet -K "${LORD_HOME}" -E "${dos_command}"
