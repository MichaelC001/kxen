#!/usr/bin/env bash
set -euo pipefail

# 本地发布路径:GitHub Actions 不可用时,在本机完成 release.yml 的 macOS arm64 build + publish 两段。
# 仅覆盖 macOS arm64;Windows/Linux/其他平台的产物只能由 release.yml 矩阵产出。
# 校验逻辑全部复用 CI 同一批脚本(validate/verify/prepare/github-release),产物走同一验证链。
#
# 本机凭证(全部在仓库外,权限 600):
#   login keychain               Developer ID Application 证书(team id 从证书自动解析)
#   ~/.tauri/kxen.key            Tauri updater 签名私钥(公钥必须与 tauri.conf.json 一致)
#   ~/.tauri/kxen.key.password   Tauri updater 签名私钥密码
#   ~/.tauri/apple-api-issuer    App Store Connect Issuer UUID
#   ~/.tauri/AuthKey_<KEY_ID>.p8 App Store Connect API key(KEY_ID 从文件名解析)
#
# 用法:
#   scripts/local-release.sh build   <tag> <owner/repo> <commit>
#   scripts/local-release.sh publish <tag> <owner/repo> <commit>

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
cd "$repo_root"

# shellcheck source=scripts/release-lib.sh
source "$script_dir/release-lib.sh"

operation="${1:-}"
release_tag="${2:-}"
repository="${3:-}"
release_commit="${4:-}"
asset_dir="$repo_root/release-assets"
bundle_root="$repo_root/src-tauri/target/aarch64-apple-darwin/release/bundle"
tauri_dir="$HOME/.tauri"

usage() {
  printf 'usage: local-release.sh <build|publish> <tag> <owner/repo> <commit>\n' >&2
  exit 1
}

[[ "$operation" == build || "$operation" == publish ]] || usage
[[ -n "$release_tag" && -n "$repository" && -n "$release_commit" ]] || usage

if [[ ! "$release_commit" =~ ^[0-9a-f]{40}$ ]]; then
  printf 'invalid release commit: %s\n' "$release_commit" >&2
  exit 1
fi

require_file() {
  local label="$1"
  local path="$2"
  if [[ ! -f "$path" ]]; then
    printf 'missing %s: %s\n' "$label" "$path" >&2
    exit 1
  fi
}

validate_source() {
  git fetch origin main --quiet
  bash "$script_dir/validate-release.sh" "$release_tag" origin/main "$repository"
}

build_assets() {
  validate_source

  require_file 'Tauri updater private key' "$tauri_dir/kxen.key"
  require_file 'Tauri updater private key password' "$tauri_dir/kxen.key.password"
  require_file 'App Store Connect issuer UUID' "$tauri_dir/apple-api-issuer"

  local p8_path
  p8_path="$(find "$tauri_dir" -maxdepth 1 -name 'AuthKey_*.p8' | head -1)"
  if [[ -z "$p8_path" ]]; then
    printf 'missing App Store Connect API key: %s/AuthKey_<KEY_ID>.p8\n' "$tauri_dir" >&2
    exit 1
  fi
  local api_key
  api_key="$(basename "$p8_path" .p8)"
  api_key="${api_key#AuthKey_}"

  local issuer
  issuer="$(tr -d '[:space:]' < "$tauri_dir/apple-api-issuer")"
  if [[ ! "$issuer" =~ ^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$ ]]; then
    printf 'APPLE_API_ISSUER must be a UUID: %s\n' "$tauri_dir/apple-api-issuer" >&2
    exit 1
  fi

  local identity_line
  identity_line="$(security find-identity -v -p codesigning | awk '/Developer ID Application/')"
  if [[ "$(printf '%s\n' "$identity_line" | awk 'NF { count += 1 } END { print count + 0 }')" != 1 ]]; then
    printf 'expected exactly one Developer ID Application identity in keychain\n' >&2
    exit 1
  fi
  local team_id
  team_id="$(sed -n 's/.*(\([A-Z0-9]\{10\}\)).*/\1/p' <<< "$identity_line")"
  if [[ ! "$team_id" =~ ^[A-Z0-9]{10}$ ]]; then
    printf 'could not parse APPLE_TEAM_ID from signing identity: %s\n' "$identity_line" >&2
    exit 1
  fi

  export TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$tauri_dir/kxen.key")"
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat "$tauri_dir/kxen.key.password")"
  export APPLE_TEAM_ID="$team_id"
  export APPLE_API_ISSUER="$issuer"
  export APPLE_API_KEY="$api_key"
  export APPLE_API_KEY_PATH="$p8_path"

  pnpm tauri build --target aarch64-apple-darwin

  # 与 release.yml 的 Notarize and staple DMG 步骤保持一致:Tauri 只公证 .app,
  # DMG 容器要过 Gatekeeper primary-signature 评估必须单独公证并 staple。
  local dmg_path
  dmg_path="$(find "$bundle_root/dmg" -maxdepth 1 -name 'Kxen_*_aarch64.dmg' | head -1)"
  if [[ -z "$dmg_path" ]]; then
    printf 'DMG bundle not found: %s\n' "$bundle_root/dmg" >&2
    exit 1
  fi
  xcrun notarytool submit "$dmg_path" \
    --key "$APPLE_API_KEY_PATH" \
    --key-id "$APPLE_API_KEY" \
    --issuer "$APPLE_API_ISSUER" \
    --wait
  xcrun stapler staple "$dmg_path"

  bash "$script_dir/verify-macos-release.sh"

  # 与 release.yml 的 Sign and notarize kxen CLI 步骤一致:同一 Developer ID 身份签名,
  # zip 提交公证后丢弃,ticket 在线生效;prepare-release-assets.sh 会再做 codesign --verify。
  cargo build --manifest-path src-tauri/Cargo.toml --release \
    -p kxen --target aarch64-apple-darwin
  local identity
  identity="$(awk '{ print $2 }' <<< "$identity_line")"
  local cli_path cli_zip_dir
  cli_path="src-tauri/target/aarch64-apple-darwin/release/kxen"
  codesign --sign "$identity" --options runtime --timestamp "$cli_path"
  codesign --verify --strict --verbose=2 "$cli_path"
  cli_zip_dir="$(mktemp -d "${TMPDIR:-/tmp}/kxen-notarize.XXXXXX")"
  ditto -c -k --keepParent "$cli_path" "$cli_zip_dir/kxen.zip"
  xcrun notarytool submit "$cli_zip_dir/kxen.zip" \
    --key "$APPLE_API_KEY_PATH" \
    --key-id "$APPLE_API_KEY" \
    --issuer "$APPLE_API_ISSUER" \
    --wait
  rm -rf "$cli_zip_dir"

  bash "$script_dir/prepare-release-assets.sh" \
    macos-aarch64 "$release_tag" "$repository" "$bundle_root" "$asset_dir"

  # 本地路径只产出 macOS arm64 一个平台,latest.json 仅含 darwin-aarch64 条目;
  # 全平台发布由 release.yml 矩阵产出,publish 段同样走这两个函数。
  kxen_merge_updater_manifest "${release_tag#v}" "$repository" "$release_tag" \
    "$asset_dir" "$asset_dir/latest.json"
  kxen_write_sha256sums "$asset_dir"
  bash "$script_dir/verify-release-assets.sh" "$release_tag" "$repository" "$asset_dir" macos-aarch64
}

publish_release() {
  validate_source
  if [[ ! -d "$asset_dir" ]] || [[ -z "$(find "$asset_dir" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
    printf 'release asset directory is empty, run the build step first: %s\n' "$asset_dir" >&2
    exit 1
  fi
  # github-release.sh 用 run id 标记 draft 归属;本地运行用 pid 占位,语义相同。
  export GITHUB_RUN_ID="$$"
  export GITHUB_RUN_ATTEMPT=1
  bash "$script_dir/github-release.sh" create-draft "$release_tag" "$repository" "$release_commit" "$asset_dir"
  bash "$script_dir/github-release.sh" verify-draft "$release_tag" "$repository" "$release_commit" "$asset_dir"
  bash "$script_dir/github-release.sh" publish "$release_tag" "$repository" "$release_commit" "$asset_dir"
}

case "$operation" in
  build) build_assets ;;
  publish) publish_release ;;
esac
