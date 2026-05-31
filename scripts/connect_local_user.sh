#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <name>" >&2
  exit 1
fi

name="$1"
key_dir="/tmp/late-sh-users"
key_path="${key_dir}/${name}"

if [[ ! -d "$key_dir" ]]; then
  mkdir -p "$key_dir"
fi

if [[ ! -f "$key_path" ]]; then
  ssh-keygen -t ed25519 -f "$key_path" -N "" -C "late-local-${name}" >/dev/null
fi

exec ssh -i "$key_path" -o IdentitiesOnly=yes -p 2222 "${name}@localhost"
