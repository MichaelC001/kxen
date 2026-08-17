#!/usr/bin/env bash
set -euo pipefail
IFS=$'\n\t'

KXEN_INSTALL_REPOSITORY="StringKe/kxen"
KXEN_INSTALL_SELF_CHECKSUM_URL="${KXEN_INSTALL_SELF_CHECKSUM_URL:-https://raw.githubusercontent.com/StringKe/kxen/main/website/public/install.sh.sha256}"
KXEN_INSTALL_SOURCE_PATH="${BASH_SOURCE[0]:-}"
KXEN_INSTALL_COMPONENT="all"
KXEN_INSTALL_VERSION="latest"
KXEN_INSTALL_DIR=""
KXEN_INSTALL_MODIFY_PATH=1
KXEN_INSTALL_PLATFORM=""
KXEN_INSTALL_ARCH=""
KXEN_INSTALL_TAG=""
KXEN_INSTALL_TEMP_DIR=""
KXEN_INSTALL_TRANSACTION_ACTIVE=0
KXEN_INSTALL_DESTINATIONS=()
KXEN_INSTALL_PENDING=()
KXEN_INSTALL_BACKUPS=()
KXEN_INSTALL_BACKUP_MOVED=()
KXEN_INSTALL_INSTALLED=()

kxen_install_usage() {
  cat <<'EOF'
kxen headless CLI installer

USAGE:
    install.sh [OPTIONS]

OPTIONS:
    --component <all|server|agent>  install both CLIs or one component (default: all)
    --all                           shorthand for --component all
    --server                        shorthand for --component server
    --agent                         shorthand for --component agent
    --version <latest|x.y.z>        install the latest stable release or an exact version
    --install-dir <PATH>            binary directory (default: ~/.local/bin)
    --no-modify-path                do not add the install directory to the user PATH
    -h, --help                      print this help

INSTALLED COMMANDS:
    server  -> kxen
    agent   -> kxen-agent
EOF
}

kxen_install_fail() {
  printf 'FAIL %s\n' "$*" >&2
  exit 1
}

kxen_install_rollback() {
  local index destination pending backup rollback_path
  for ((index = ${#KXEN_INSTALL_DESTINATIONS[@]} - 1; index >= 0; index--)); do
    destination="${KXEN_INSTALL_DESTINATIONS[$index]:-}"
    pending="${KXEN_INSTALL_PENDING[$index]:-}"
    backup="${KXEN_INSTALL_BACKUPS[$index]:-}"
    if [[ "${KXEN_INSTALL_INSTALLED[$index]:-0}" == 1 && -n "$destination" && ( -e "$destination" || -L "$destination" ) ]]; then
      rollback_path="$KXEN_INSTALL_TEMP_DIR/rollback-$index"
      mv "$destination" "$rollback_path" 2>/dev/null || true
    fi
    if [[ "${KXEN_INSTALL_BACKUP_MOVED[$index]:-0}" == 1 && -n "$backup" && ( -e "$backup" || -L "$backup" ) ]]; then
      mv "$backup" "$destination" 2>/dev/null || true
    fi
    if [[ -n "$pending" && ( -e "$pending" || -L "$pending" ) ]]; then
      mv "$pending" "$KXEN_INSTALL_TEMP_DIR/pending-$index" 2>/dev/null || true
    fi
  done
}

kxen_install_on_exit() {
  local status=$?
  local index pending backup
  trap - EXIT INT TERM
  if [[ "$KXEN_INSTALL_TRANSACTION_ACTIVE" == 1 ]]; then
    kxen_install_rollback
  elif [[ "$status" != 0 ]]; then
    for ((index = 0; index < ${#KXEN_INSTALL_PENDING[@]}; index++)); do
      pending="${KXEN_INSTALL_PENDING[$index]:-}"
      backup="${KXEN_INSTALL_BACKUPS[$index]:-}"
      if [[ -n "$pending" && ( -e "$pending" || -L "$pending" ) ]]; then
        mv "$pending" "$KXEN_INSTALL_TEMP_DIR/pending-$index" 2>/dev/null || true
      fi
      if [[ -n "$backup" && ( -e "$backup" || -L "$backup" ) ]]; then
        printf 'WARN previous binary backup retained at %s\n' "$backup" >&2
      fi
    done
  fi
  if [[ -n "$KXEN_INSTALL_TEMP_DIR" && -d "$KXEN_INSTALL_TEMP_DIR" ]]; then
    rm -rf "$KXEN_INSTALL_TEMP_DIR"
  fi
  exit "$status"
}

kxen_install_parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --component)
        [[ $# -ge 2 ]] || kxen_install_fail '--component requires a value'
        KXEN_INSTALL_COMPONENT="$2"
        shift 2
        ;;
      --component=*)
        KXEN_INSTALL_COMPONENT="${1#*=}"
        shift
        ;;
      --all)
        KXEN_INSTALL_COMPONENT="all"
        shift
        ;;
      --server)
        KXEN_INSTALL_COMPONENT="server"
        shift
        ;;
      --agent)
        KXEN_INSTALL_COMPONENT="agent"
        shift
        ;;
      --version)
        [[ $# -ge 2 ]] || kxen_install_fail '--version requires a value'
        KXEN_INSTALL_VERSION="$2"
        shift 2
        ;;
      --version=*)
        KXEN_INSTALL_VERSION="${1#*=}"
        shift
        ;;
      --install-dir)
        [[ $# -ge 2 ]] || kxen_install_fail '--install-dir requires a value'
        KXEN_INSTALL_DIR="$2"
        shift 2
        ;;
      --install-dir=*)
        KXEN_INSTALL_DIR="${1#*=}"
        shift
        ;;
      --no-modify-path)
        KXEN_INSTALL_MODIFY_PATH=0
        shift
        ;;
      -h | --help)
        kxen_install_usage
        exit 0
        ;;
      *)
        kxen_install_fail "unknown argument: $1"
        ;;
    esac
  done
  case "$KXEN_INSTALL_COMPONENT" in
    all | server | agent) ;;
    *) kxen_install_fail "invalid component: $KXEN_INSTALL_COMPONENT" ;;
  esac
}

kxen_install_validate_tag() {
  local tag="$1"
  [[ "$tag" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]
}

kxen_install_normalize_tag() {
  local version="$1"
  local tag
  case "$version" in
    v*) tag="$version" ;;
    *) tag="v$version" ;;
  esac
  kxen_install_validate_tag "$tag" || return 1
  printf '%s\n' "$tag"
}

kxen_install_download() {
  local url="$1"
  local output="$2"
  curl \
    --proto '=https' \
    --tlsv1.2 \
    --fail \
    --silent \
    --show-error \
    --location \
    --retry 3 \
    --retry-delay 1 \
    --connect-timeout 15 \
    --max-time 180 \
    --output "$output" \
    "$url"
}

kxen_install_resolve_tag() {
  local release_json tag
  if [[ "$KXEN_INSTALL_VERSION" != latest ]]; then
    KXEN_INSTALL_TAG="$(kxen_install_normalize_tag "$KXEN_INSTALL_VERSION")" ||
      kxen_install_fail "invalid stable version: $KXEN_INSTALL_VERSION"
    return
  fi
  release_json="$KXEN_INSTALL_TEMP_DIR/latest-release.json"
  kxen_install_download \
    "https://api.github.com/repos/$KXEN_INSTALL_REPOSITORY/releases/latest" \
    "$release_json"
  tag="$(sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$release_json" | sed -n '1p')"
  kxen_install_validate_tag "$tag" || kxen_install_fail 'GitHub latest release did not return a stable SemVer tag'
  KXEN_INSTALL_TAG="$tag"
}

kxen_install_map_arch() {
  case "$1" in
    x86_64 | amd64 | AMD64 | X64) printf 'x86_64\n' ;;
    aarch64 | arm64 | ARM64 | Arm64) printf 'aarch64\n' ;;
    *) return 1 ;;
  esac
}

kxen_install_detect_platform() {
  local kernel raw_arch translated macos_major libc_description
  kernel="$(uname -s)"
  raw_arch="$(uname -m)"
  case "$kernel" in
    Darwin)
      KXEN_INSTALL_PLATFORM="macos"
      macos_major="$(sw_vers -productVersion | cut -d. -f1)"
      [[ "$macos_major" =~ ^[0-9]+$ ]] || kxen_install_fail 'unable to determine the macOS version'
      ((macos_major >= 14)) || kxen_install_fail 'kxen requires macOS 14 or newer'
      if [[ "$raw_arch" == x86_64 ]]; then
        translated="$(sysctl -in sysctl.proc_translated 2>/dev/null || true)"
        if [[ "$translated" == 1 ]]; then
          raw_arch="arm64"
        fi
      fi
      ;;
    Linux)
      KXEN_INSTALL_PLATFORM="linux"
      if command -v ldd >/dev/null 2>&1; then
        libc_description="$(ldd --version 2>&1 || true)"
        if printf '%s\n' "$libc_description" | tr '[:upper:]' '[:lower:]' | grep -q musl; then
          kxen_install_fail 'published Linux binaries require glibc; musl and Alpine are not supported'
        fi
      fi
      ;;
    MINGW* | MSYS* | CYGWIN*)
      kxen_install_fail 'native Windows installation requires https://kxen.ai/install.ps1'
      ;;
    *)
      kxen_install_fail "unsupported operating system: $kernel"
      ;;
  esac
  KXEN_INSTALL_ARCH="$(kxen_install_map_arch "$raw_arch")" ||
    kxen_install_fail "unsupported architecture: $raw_arch"
}

kxen_install_asset_name() {
  local component="$1"
  local platform="$2"
  local arch="$3"
  case "$component" in
    server) printf 'kxen-%s-%s.tar.gz\n' "$platform" "$arch" ;;
    agent) printf 'kxen-agent-%s-%s.tar.gz\n' "$platform" "$arch" ;;
    *) return 1 ;;
  esac
}

kxen_install_binary_name() {
  case "$1" in
    server) printf 'kxen\n' ;;
    agent) printf 'kxen-agent\n' ;;
    *) return 1 ;;
  esac
}

kxen_install_expected_checksum() {
  local checksums="$1"
  local asset="$2"
  local matches count checksum
  matches="$(awk -v asset="$asset" '$2 == asset { print $1 }' "$checksums")"
  count="$(printf '%s\n' "$matches" | awk 'NF { count += 1 } END { print count + 0 }')"
  [[ "$count" == 1 ]] || return 1
  checksum="$(printf '%s\n' "$matches" | sed -n '1p')"
  [[ "$checksum" =~ ^[0-9A-Fa-f]{64}$ ]] || return 1
  printf '%s\n' "$checksum" | tr '[:upper:]' '[:lower:]'
}

kxen_install_actual_checksum() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{ print tolower($1) }'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{ print tolower($1) }'
  else
    kxen_install_fail 'sha256sum or shasum is required'
  fi
}

kxen_install_verify_checksum() {
  local checksums="$1"
  local asset="$2"
  local path="$3"
  local expected actual
  expected="$(kxen_install_expected_checksum "$checksums" "$asset")" ||
    kxen_install_fail "SHA256SUMS does not contain exactly one valid entry for $asset"
  actual="$(kxen_install_actual_checksum "$path")"
  [[ "$actual" == "$expected" ]] || kxen_install_fail "SHA-256 mismatch for $asset"
  printf 'PASS SHA-256 %s\n' "$asset"
}

kxen_install_verify_self() {
  local checksums expected actual
  if [[ -z "$KXEN_INSTALL_SOURCE_PATH" || ! -f "$KXEN_INSTALL_SOURCE_PATH" ]]; then
    printf 'WARN installer self-check unavailable for piped input; HTTPS is the only script integrity boundary\n' >&2
    return
  fi
  checksums="$KXEN_INSTALL_TEMP_DIR/install.sh.sha256"
  kxen_install_download "$KXEN_INSTALL_SELF_CHECKSUM_URL" "$checksums"
  expected="$(kxen_install_expected_checksum "$checksums" install.sh)" ||
    kxen_install_fail 'installer checksum source does not contain exactly one valid entry for install.sh'
  actual="$(kxen_install_actual_checksum "$KXEN_INSTALL_SOURCE_PATH")"
  [[ "$actual" == "$expected" ]] || kxen_install_fail 'SHA-256 mismatch for install.sh'
  printf 'PASS SHA-256 install.sh\n'
}

kxen_install_extract_asset() {
  local archive="$1"
  local binary="$2"
  local output_dir="$3"
  local entries
  entries="$(tar -tzf "$archive" | sed 's#^\./##')" || kxen_install_fail "unable to inspect $(basename "$archive")"
  [[ "$entries" == "$binary" ]] || kxen_install_fail "archive must contain only $binary: $(basename "$archive")"
  mkdir -p "$output_dir"
  tar -xzf "$archive" -C "$output_dir"
  [[ -f "$output_dir/$binary" && ! -L "$output_dir/$binary" ]] ||
    kxen_install_fail "archive did not produce a regular $binary file"
  chmod 0755 "$output_dir/$binary"
}

kxen_install_smoke_binary() {
  local component="$1"
  local binary_path="$2"
  local expected_version="${KXEN_INSTALL_TAG#v}"
  local version_output
  if [[ "$component" == server ]]; then
    "$binary_path" --help >/dev/null
    return
  fi
  version_output="$("$binary_path" --version)"
  [[ "$version_output" == "kxen-agent $expected_version" ]] ||
    kxen_install_fail "kxen-agent version mismatch: expected $expected_version, got $version_output"
}

kxen_install_shell_path_line() {
  local path="$1"
  local escaped
  escaped="$(printf '%s' "$path" | sed "s/'/'\\\\''/g")"
  printf '%s\n' "export PATH='$escaped':\"\$PATH\""
}

kxen_install_path_contains() {
  case ":${PATH:-}:" in
    *":$1:"*) return 0 ;;
    *) return 1 ;;
  esac
}

kxen_install_ensure_path() {
  local shell_name profile line
  if kxen_install_path_contains "$KXEN_INSTALL_DIR"; then
    printf 'PASS PATH already contains %s\n' "$KXEN_INSTALL_DIR"
    return
  fi
  line="$(kxen_install_shell_path_line "$KXEN_INSTALL_DIR")"
  if [[ "$KXEN_INSTALL_MODIFY_PATH" == 0 ]]; then
    printf 'PATH not modified. Run now and add to your shell profile:\n  %s\n' "$line"
    return
  fi
  [[ -n "${HOME:-}" ]] || kxen_install_fail 'HOME is required to update PATH'
  shell_name="$(basename "${SHELL:-sh}")"
  case "$shell_name" in
    zsh) profile="$HOME/.zshrc" ;;
    bash)
      if [[ "$KXEN_INSTALL_PLATFORM" == macos ]]; then
        profile="$HOME/.bash_profile"
      else
        profile="$HOME/.bashrc"
      fi
      ;;
    sh | dash | ksh) profile="$HOME/.profile" ;;
    fish)
      printf 'PATH was not modified for fish. Run:\n  fish_add_path %s\n' "$KXEN_INSTALL_DIR"
      return
      ;;
    *) profile="$HOME/.profile" ;;
  esac
  if [[ -e "$profile" && ! -f "$profile" ]]; then
    kxen_install_fail "shell profile is not a regular file: $profile"
  fi
  if ! grep -Fqx "$line" "$profile" 2>/dev/null; then
    printf '\n# kxen CLI\n%s\n' "$line" >> "$profile"
    printf 'PASS added %s to PATH in %s\n' "$KXEN_INSTALL_DIR" "$profile"
  else
    printf 'PASS PATH profile already contains %s\n' "$KXEN_INSTALL_DIR"
  fi
  printf 'Open a new shell or run:\n  %s\n' "$line"
}

kxen_install_commit_binaries() {
  local staged_root="$1"
  shift
  local components=("$@")
  local index component binary source destination pending backup
  mkdir -p "$KXEN_INSTALL_DIR"
  [[ -d "$KXEN_INSTALL_DIR" && -w "$KXEN_INSTALL_DIR" ]] ||
    kxen_install_fail "install directory is not writable: $KXEN_INSTALL_DIR"

  for ((index = 0; index < ${#components[@]}; index++)); do
    component="${components[$index]}"
    binary="$(kxen_install_binary_name "$component")"
    source="$staged_root/$component/$binary"
    destination="$KXEN_INSTALL_DIR/$binary"
    if [[ -d "$destination" && ! -L "$destination" ]]; then
      kxen_install_fail "destination is a directory: $destination"
    fi
    pending="$(mktemp "$KXEN_INSTALL_DIR/.${binary}.kxen-install.XXXXXX")"
    backup="$pending.backup"
    [[ ! -e "$backup" && ! -L "$backup" ]] || kxen_install_fail "backup path already exists: $backup"
    KXEN_INSTALL_DESTINATIONS[index]="$destination"
    KXEN_INSTALL_PENDING[index]="$pending"
    KXEN_INSTALL_BACKUPS[index]="$backup"
    KXEN_INSTALL_BACKUP_MOVED[index]=0
    KXEN_INSTALL_INSTALLED[index]=0
    cp "$source" "$pending"
    chmod 0755 "$pending"
  done

  KXEN_INSTALL_TRANSACTION_ACTIVE=1
  for ((index = 0; index < ${#components[@]}; index++)); do
    destination="${KXEN_INSTALL_DESTINATIONS[$index]}"
    pending="${KXEN_INSTALL_PENDING[$index]}"
    backup="${KXEN_INSTALL_BACKUPS[$index]}"
    if [[ -e "$destination" || -L "$destination" ]]; then
      mv "$destination" "$backup"
      KXEN_INSTALL_BACKUP_MOVED[index]=1
    fi
    mv "$pending" "$destination"
    KXEN_INSTALL_INSTALLED[index]=1
  done

  kxen_install_ensure_path

  KXEN_INSTALL_TRANSACTION_ACTIVE=0
  for ((index = 0; index < ${#components[@]}; index++)); do
    backup="${KXEN_INSTALL_BACKUPS[$index]}"
    if [[ -e "$backup" || -L "$backup" ]]; then
      if ! mv "$backup" "$KXEN_INSTALL_TEMP_DIR/previous-$index"; then
        printf 'WARN previous binary backup retained at %s\n' "$backup" >&2
      fi
    fi
  done
}

kxen_install_main() {
  local components=()
  local component binary asset archive extract_dir checksums release_base

  kxen_install_parse_args "$@"
  command -v curl >/dev/null 2>&1 || kxen_install_fail 'curl is required'
  command -v tar >/dev/null 2>&1 || kxen_install_fail 'tar is required'
  command -v mktemp >/dev/null 2>&1 || kxen_install_fail 'mktemp is required'
  [[ -n "${HOME:-}" || -n "$KXEN_INSTALL_DIR" ]] || kxen_install_fail 'HOME or --install-dir is required'
  if [[ -z "$KXEN_INSTALL_DIR" ]]; then
    KXEN_INSTALL_DIR="$HOME/.local/bin"
  fi
  case "$KXEN_INSTALL_DIR" in
    /*) ;;
    *) kxen_install_fail "--install-dir must be absolute: $KXEN_INSTALL_DIR" ;;
  esac

  KXEN_INSTALL_TEMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/kxen-install.XXXXXX")"
  trap kxen_install_on_exit EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  kxen_install_verify_self
  kxen_install_detect_platform
  kxen_install_resolve_tag

  case "$KXEN_INSTALL_COMPONENT" in
    all) components=(server agent) ;;
    server) components=(server) ;;
    agent) components=(agent) ;;
  esac

  printf 'Installing Kxen %s for %s-%s into %s\n' \
    "$KXEN_INSTALL_TAG" "$KXEN_INSTALL_PLATFORM" "$KXEN_INSTALL_ARCH" "$KXEN_INSTALL_DIR"
  release_base="https://github.com/$KXEN_INSTALL_REPOSITORY/releases/download/$KXEN_INSTALL_TAG"
  checksums="$KXEN_INSTALL_TEMP_DIR/SHA256SUMS"
  kxen_install_download "$release_base/SHA256SUMS" "$checksums"

  for component in "${components[@]}"; do
    binary="$(kxen_install_binary_name "$component")"
    asset="$(kxen_install_asset_name "$component" "$KXEN_INSTALL_PLATFORM" "$KXEN_INSTALL_ARCH")"
    archive="$KXEN_INSTALL_TEMP_DIR/$asset"
    extract_dir="$KXEN_INSTALL_TEMP_DIR/staged/$component"
    kxen_install_download "$release_base/$asset" "$archive"
    kxen_install_verify_checksum "$checksums" "$asset" "$archive"
    kxen_install_extract_asset "$archive" "$binary" "$extract_dir"
    if [[ "$KXEN_INSTALL_PLATFORM" == macos ]]; then
      codesign --verify --strict --verbose=2 "$extract_dir/$binary"
    fi
    kxen_install_smoke_binary "$component" "$extract_dir/$binary"
    printf 'PASS verified %s\n' "$binary"
  done

  kxen_install_commit_binaries "$KXEN_INSTALL_TEMP_DIR/staged" "${components[@]}"
  for component in "${components[@]}"; do
    binary="$(kxen_install_binary_name "$component")"
    printf 'PASS installed %s\n' "$KXEN_INSTALL_DIR/$binary"
  done
}

if [[ -z "${BASH_SOURCE[0]:-}" || "${BASH_SOURCE[0]}" == "$0" ]]; then
  kxen_install_main "$@"
fi
