#!/usr/bin/env bash

set -euo pipefail

readonly LATE_BIN_NAME="late"
readonly LATE_DEFAULT_BASE_URL="https://cli.late.sh"
# Checksums are read from the GitHub Release, never from the download host:
# a binary served by cli.late.sh must match a sha256sums.txt served by
# GitHub, so swapping files on one origin is not enough to ship a different
# binary. That holds for a trusted copy of this script; the script itself is
# also served from cli.late.sh, so `curl ... | sh` trusts the host for the
# installer. Every release also carries a Sigstore provenance bundle next to
# each binary (<binary>.sigstore.json); see late-cli/README.md to verify it.
readonly LATE_DEFAULT_CHECKSUM_BASE_URL="https://github.com/mpiorowski/late-sh/releases/download"
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

is_windows_shell() {
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
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
    MINGW*|MSYS*|CYGWIN*)
      if [[ "$arch" != "x86_64" ]]; then
        fail "unsupported Windows architecture: $arch (native ARM64 build is not published yet)"
      fi
      printf '%s\n' "x86_64-pc-windows-msvc"
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

# Verification fails closed: no checksum entry, no SHA-256 tool, or a
# mismatch all abort the install rather than installing an unverified file.
verify_checksum() {
  local checksum_file="$1"
  local target="$2"
  local downloaded_file="$3"
  local binary_name="$4"
  local expected actual cmd

  expected="$(awk -v path="${target}/${binary_name}" '$2 == path { print $1 }' "$checksum_file")"
  [[ -n "$expected" ]] || fail "missing checksum for ${target}/${binary_name}"

  cmd="$(checksum_cmd)"
  [[ -n "$cmd" ]] || fail "no SHA-256 tool found (sha256sum or shasum); refusing to install an unverified ${binary_name}"

  actual="$($cmd "$downloaded_file" | awk '{ print $1 }')"
  [[ "$actual" == "$expected" ]] || fail "checksum mismatch for ${binary_name}"
  log_verbose "verified sha256 of ${binary_name}: ${actual}"
}

# Release tags are used verbatim in URL paths on two hosts, so a value that
# came from the network is only accepted if it looks like a tag. Without this
# a tampered VERSION file could point the checksum URL at another GitHub path.
validate_version() {
  local version="$1"
  local source="$2"

  [[ "$version" =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || fail "unexpected version string from ${source}: ${version}"
}

# `latest` is a pointer, not a release. It resolves to the tag in
# {base}/latest/VERSION and everything after that (binary, helper, checksums)
# is fetched for that exact tag, so a publish in progress cannot mix files
# from two versions.
resolve_version() {
  local base_url="$1"
  local requested="$2"
  local version_url version

  case "$requested" in
    latest)
      version_url="${base_url%/}/latest/VERSION"
      version="$(curl -fsSL "$version_url" | tr -d '[:space:]')" || fail "could not resolve the latest version from ${version_url}"
      [[ -n "$version" ]] || fail "empty version file at ${version_url}"
      validate_version "$version" "$version_url"
      ;;
    *)
      version="$requested"
      validate_version "$version" "LATE_INSTALL_VERSION"
      ;;
  esac

  printf '%s\n' "$version"
}

# Desktop Linux ships a second binary: the late-webview YouTube helper.
# `late` itself does not link WebKitGTK, so the CLI runs without the webview
# libraries; the helper only needs them when embedded YouTube playback is
# used. Missing helper downloads are warning-only so older releases still
# install.
install_webview_helper() {
  local base_url="$1"
  local prefix="$2"
  local target="$3"
  local tmp_dir="$4"
  local target_dir="$5"
  local helper_name="late-webview"
  local helper_url dest_path

  case "$target" in
    *-unknown-linux-gnu) ;;
    *) return ;;
  esac

  helper_url="${base_url%/}/${prefix}/${target}/${helper_name}"
  log_verbose "helper_url=${helper_url}"
  if ! curl -fsSL "$helper_url" -o "${tmp_dir}/${helper_name}"; then
    log "warning: ${helper_name} unavailable at ${helper_url}; embedded YouTube playback stays disabled"
    return
  fi
  chmod 755 "${tmp_dir}/${helper_name}"
  verify_checksum "${tmp_dir}/sha256sums.txt" "$target" "${tmp_dir}/${helper_name}" "$helper_name"

  dest_path="$(install_binary "${tmp_dir}/${helper_name}" "$target_dir" "$helper_name")"
  log "installed ${helper_name} to ${dest_path}"
}

install_binary() {
  local src="$1"
  local dest_dir="$2"
  local dest_name="${3:-$LATE_BIN_NAME}"
  local dest_path="${dest_dir}/${dest_name}"

  mkdir -p "$dest_dir"

  if command -v install >/dev/null 2>&1; then
    install -m 755 "$src" "$dest_path"
  else
    cp "$src" "$dest_path"
    chmod 755 "$dest_path"
  fi

  printf '%s\n' "$dest_path"
}

normalize_shell_path() {
  local path="$1"

  if is_windows_shell && command -v cygpath >/dev/null 2>&1; then
    cygpath -u "$path"
    return
  fi

  printf '%s\n' "$path"
}

windows_install_target_dir() {
  local local_app_data

  if [[ -n "${LOCALAPPDATA:-}" ]]; then
    local_app_data="$(normalize_shell_path "$LOCALAPPDATA")"
  elif [[ -n "${USERPROFILE:-}" ]]; then
    local_app_data="$(normalize_shell_path "${USERPROFILE}\\AppData\\Local")"
  else
    local_app_data="${HOME}/AppData/Local"
  fi

  printf '%s\n' "${local_app_data}/Programs/late"
}

install_target_dir() {
  local target="$1"

  if [[ -n "${LATE_INSTALL_DIR:-}" ]]; then
    normalize_shell_path "$LATE_INSTALL_DIR"
  elif [[ "$target" == *-windows-* ]]; then
    windows_install_target_dir
  elif is_termux; then
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
  LATE_INSTALL_BASE_URL            Override distribution host
  LATE_INSTALL_CHECKSUM_BASE_URL   Override where sha256sums.txt is read from
                                   (default: the GitHub Release assets)
  LATE_INSTALL_VERSION             Use a specific version instead of latest
  LATE_INSTALL_DIR                 Override the install directory
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
  local base_url checksum_base_url version prefix target tmp_dir binary_url checksum_url target_dir dest_path binary_name

  parse_args "$@"
  need_cmd curl
  need_cmd uname
  need_cmd mktemp

  base_url="${LATE_INSTALL_BASE_URL:-$LATE_DEFAULT_BASE_URL}"
  checksum_base_url="${LATE_INSTALL_CHECKSUM_BASE_URL:-$LATE_DEFAULT_CHECKSUM_BASE_URL}"
  version="$(resolve_version "$base_url" "${LATE_INSTALL_VERSION:-latest}")"
  target="$(detect_target)"
  binary_name="$(binary_name_for_target "$target")"

  if is_wsl; then
    log "detected WSL; installing the Linux build"
  elif is_termux; then
    log "detected Termux; installing the Android build"
  elif [[ "$target" == *-windows-* ]]; then
    log "detected Windows shell; installing the Windows build"
  fi

  prefix="releases/${version}"

  tmp_dir="$(mktemp -d)"
  trap "rm -rf '$tmp_dir'" EXIT

  binary_url="${base_url%/}/${prefix}/${target}/${binary_name}"
  checksum_url="${checksum_base_url%/}/${version}/sha256sums.txt"

  log_verbose "base_url=${base_url}"
  log_verbose "version=${version}"
  log_verbose "target=${target}"
  log_verbose "binary_url=${binary_url}"
  log_verbose "checksum_url=${checksum_url}"

  log "fetching checksums for ${version} from ${checksum_url}"
  curl -fsSL "$checksum_url" -o "${tmp_dir}/sha256sums.txt" \
    || fail "checksum file unavailable at ${checksum_url}; refusing to install an unverified binary"

  log "downloading ${target} from ${binary_url}"
  curl -fsSL "$binary_url" -o "${tmp_dir}/${binary_name}"
  chmod 755 "${tmp_dir}/${binary_name}"
  verify_checksum "${tmp_dir}/sha256sums.txt" "$target" "${tmp_dir}/${binary_name}" "${binary_name}"

  target_dir="$(install_target_dir "$target")"

  log_verbose "target_dir=${target_dir}"
  dest_path="$(install_binary "${tmp_dir}/${binary_name}" "$target_dir" "$binary_name")"
  log "installed ${binary_name} to ${dest_path}"

  install_webview_helper "$base_url" "$prefix" "$target" "$tmp_dir" "$target_dir"

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

  if [[ "$target" == *-windows-* ]]; then
    log "run 'late.exe --help' to verify the install"
  else
    log "run 'late --help' to verify the install"
  fi
}

main "$@"
