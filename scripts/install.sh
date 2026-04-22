#!/usr/bin/env bash

set -euo pipefail

readonly LATE_BIN_NAME="late"
readonly LATE_DEFAULT_BASE_URL="https://cli.late.sh"
VERBOSE=0

log() {
  printf 'late installer: %s\n' "$*"
}

log_verbose() {
  if [[ "$VERBOSE" -eq 1 ]]; then
    printf 'late installer: %s\n' "$*"
  fi
}

fail() {
  printf 'late installer: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

is_wsl() {
  [[ -r /proc/sys/kernel/osrelease ]] && grep -qi microsoft /proc/sys/kernel/osrelease
}

is_termux() {
  [[ -n "${TERMUX_VERSION:-}" ]] || [[ "${PREFIX:-}" == "/data/data/com.termux/files/usr" ]]
}

detect_target() {
  local os arch

  os="$(uname -s)"
  arch="$(uname -m)"

  case "$arch" in
    x86_64|amd64)
      arch="x86_64"
      ;;
    arm64|aarch64)
      arch="aarch64"
      ;;
    *)
      fail "unsupported architecture: $arch"
      ;;
  esac

  case "$os" in
    Linux)
      if is_termux; then
        printf '%s\n' "${arch}-linux-android"
      else
        printf '%s\n' "${arch}-unknown-linux-gnu"
      fi
      ;;
    Darwin)
      printf '%s\n' "${arch}-apple-darwin"
      ;;
    *)
      fail "unsupported operating system: $os"
      ;;
  esac
}

checksum_cmd() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf '%s\n' "sha256sum"
    return
  fi

  if command -v shasum >/dev/null 2>&1; then
    printf '%s\n' "shasum -a 256"
    return
  fi

  printf '%s\n' ""
}

binary_name_for_target() {
  local target="$1"

  case "$target" in
    *-windows-*)
      printf '%s\n' "late.exe"
      ;;
    *)
      printf '%s\n' "${LATE_BIN_NAME}"
      ;;
  esac
}

verify_checksum() {
  local checksum_file="$1"
  local target="$2"
  local downloaded_file="$3"
  local binary_name="$4"
  local expected actual cmd

  expected="$(awk -v path="${target}/${binary_name}" '$2 == path { print $1 }' "$checksum_file")"
  [[ -n "$expected" ]] || fail "missing checksum for ${target}/${binary_name}"

  cmd="$(checksum_cmd)"
  if [[ -z "$cmd" ]]; then
    log "warning: no SHA-256 tool found; skipping checksum verification"
    return
  fi

  actual="$($cmd "$downloaded_file" | awk '{ print $1 }')"
  [[ "$actual" == "$expected" ]] || fail "checksum mismatch for ${LATE_BIN_NAME}"
}

install_binary() {
  local src="$1"
  local dest_dir="$2"
  local dest_path="${dest_dir}/${LATE_BIN_NAME}"

  mkdir -p "$dest_dir"

  if command -v install >/dev/null 2>&1; then
    install -m 755 "$src" "$dest_path"
  else
    cp "$src" "$dest_path"
    chmod 755 "$dest_path"
  fi

  printf '%s\n' "$dest_path"
}

install_target_dir() {
  if is_termux; then
    printf '%s\n' "${PREFIX:-/data/data/com.termux/files/usr}/bin"
  elif [[ "${EUID:-$(id -u)}" -eq 0 ]]; then
    printf '%s\n' "/usr/local/bin"
  else
    printf '%s\n' "${HOME}/.local/bin"
  fi
}

shell_rc_path() {
  local shell_name
  shell_name="$(basename "${SHELL:-}")"

  case "$shell_name" in
    zsh)
      if [[ -f "${HOME}/.zprofile" || ! -f "${HOME}/.zshrc" ]]; then
        printf '%s\n' "${HOME}/.zprofile"
      else
        printf '%s\n' "${HOME}/.zshrc"
      fi
      ;;
    bash)
      if [[ -f "${HOME}/.bash_profile" || ! -f "${HOME}/.bashrc" ]]; then
        printf '%s\n' "${HOME}/.bash_profile"
      else
        printf '%s\n' "${HOME}/.bashrc"
      fi
      ;;
    *)
      printf '%s\n' "${HOME}/.profile"
      ;;
  esac
}

ensure_path_in_shell_rc() {
  local target_dir="$1"
  local rc_file path_line

  rc_file="$(shell_rc_path)"
  path_line="export PATH=\"${target_dir}:\$PATH\""

  mkdir -p "$(dirname "$rc_file")"
  touch "$rc_file"

  if grep -Fqs "$path_line" "$rc_file"; then
    log "PATH entry already present in ${rc_file}"
    return
  fi

  {
    printf '\n'
    printf '# Added by late installer\n'
    printf '%s\n' "$path_line"
  } >>"$rc_file"

  log "added ${target_dir} to PATH in ${rc_file}"
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --verbose|-v)
        VERBOSE=1
        ;;
      --help|-h)
        cat <<'EOF'
late installer

Options:
  -v, --verbose   Print resolved target, URLs, and install paths
  -h, --help      Show this help

Environment:
  LATE_INSTALL_BASE_URL   Override distribution host
  LATE_INSTALL_VERSION    Use a specific version instead of latest
EOF
        exit 0
        ;;
      *)
        fail "unknown argument: $1"
        ;;
    esac
    shift
  done
}

main() {
  local base_url version prefix target tmp_dir binary_url checksum_url target_dir dest_path binary_name

  parse_args "$@"
  need_cmd curl
  need_cmd uname
  need_cmd mktemp

  base_url="${LATE_INSTALL_BASE_URL:-$LATE_DEFAULT_BASE_URL}"
  version="${LATE_INSTALL_VERSION:-latest}"
  target="$(detect_target)"
  binary_name="$(binary_name_for_target "$target")"

  if is_wsl; then
    log "detected WSL; installing the Linux build"
  elif is_termux; then
    log "detected Termux; installing the Android build"
  fi

  case "$version" in
    latest)
      prefix="latest"
      ;;
    *)
      prefix="releases/${version}"
      ;;
  esac

  tmp_dir="$(mktemp -d)"
  trap "rm -rf '$tmp_dir'" EXIT

  binary_url="${base_url%/}/${prefix}/${target}/${binary_name}"
  checksum_url="${base_url%/}/${prefix}/sha256sums.txt"

  log_verbose "base_url=${base_url}"
  log_verbose "version=${version}"
  log_verbose "target=${target}"
  log_verbose "binary_url=${binary_url}"
  log_verbose "checksum_url=${checksum_url}"
  log "downloading ${target} from ${binary_url}"
  curl -fsSL "$binary_url" -o "${tmp_dir}/${binary_name}"
  chmod 755 "${tmp_dir}/${binary_name}"

  if curl -fsSL "$checksum_url" -o "${tmp_dir}/sha256sums.txt"; then
    verify_checksum "${tmp_dir}/sha256sums.txt" "$target" "${tmp_dir}/${binary_name}" "${binary_name}"
  else
    log "warning: checksum file unavailable at ${checksum_url}; continuing without verification"
  fi

  target_dir="$(install_target_dir)"

  log_verbose "target_dir=${target_dir}"
  dest_path="$(install_binary "${tmp_dir}/${binary_name}" "$target_dir")"
  log "installed ${LATE_BIN_NAME} to ${dest_path}"

  case ":${PATH}:" in
    *":${target_dir}:"*)
      ;;
    *)
      if [[ "${EUID:-$(id -u)}" -ne 0 ]] && ! is_termux; then
        ensure_path_in_shell_rc "$target_dir"
      else
        log "warning: ${target_dir} is not currently on PATH"
      fi
      ;;
  esac

  log "run 'late --help' to verify the install"
}

main "$@"
