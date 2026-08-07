#!/usr/bin/env bash
# 发布矩阵单一出处:平台 / runner / rust target / 稳定 asset 命名。
# release.yml 的 build 矩阵由 `release-manifest.sh json` 派生;
# prepare-release-assets.sh 与 verify-release-assets.sh source 本文件取命名规则。
# 稳定 asset 名与 docs/web-mode/design.md 第 6 节一致,改名必须同步设计文档。

KXEN_RELEASE_PLATFORMS=(macos-aarch64 macos-x86_64 linux-x86_64 linux-aarch64 windows-x86_64 windows-aarch64)

kxen_release_platform_exists() {
  local platform="$1"
  local known
  for known in "${KXEN_RELEASE_PLATFORMS[@]}"; do
    if [[ "$known" == "$platform" ]]; then
      return 0
    fi
  done
  printf 'unknown release platform: %s\n' "$platform" >&2
  return 1
}

kxen_release_runner() {
  case "$1" in
    macos-aarch64) printf 'macos-14\n' ;;
    macos-x86_64) printf 'macos-15-intel\n' ;;
    linux-x86_64) printf 'ubuntu-22.04\n' ;;
    linux-aarch64) printf 'ubuntu-22.04-arm\n' ;;
    windows-x86_64) printf 'windows-2025\n' ;;
    windows-aarch64) printf 'windows-11-arm\n' ;;
    *) return 1 ;;
  esac
}

kxen_release_target() {
  case "$1" in
    macos-aarch64) printf 'aarch64-apple-darwin\n' ;;
    macos-x86_64) printf 'x86_64-apple-darwin\n' ;;
    linux-x86_64) printf 'x86_64-unknown-linux-gnu\n' ;;
    linux-aarch64) printf 'aarch64-unknown-linux-gnu\n' ;;
    windows-x86_64) printf 'x86_64-pc-windows-msvc\n' ;;
    windows-aarch64) printf 'aarch64-pc-windows-msvc\n' ;;
    *) return 1 ;;
  esac
}

kxen_release_os() {
  case "$1" in
    macos-*) printf 'macos\n' ;;
    linux-*) printf 'linux\n' ;;
    windows-*) printf 'windows\n' ;;
    *) return 1 ;;
  esac
}

# Apple Developer ID 签名 + 公证;其余平台无 OS 级签名(updater minisign 签名全平台都有)。
kxen_release_signed() {
  case "$1" in
    macos-*) printf 'true\n' ;;
    linux-* | windows-*) printf 'false\n' ;;
    *) return 1 ;;
  esac
}

# tauri updater platform key(deb 不进 updater,每平台恰好一个 updater 条目)。
kxen_release_updater_key() {
  case "$1" in
    macos-aarch64) printf 'darwin-aarch64\n' ;;
    macos-x86_64) printf 'darwin-x86_64\n' ;;
    linux-x86_64) printf 'linux-x86_64\n' ;;
    linux-aarch64) printf 'linux-aarch64\n' ;;
    windows-x86_64) printf 'windows-x86_64\n' ;;
    windows-aarch64) printf 'windows-aarch64\n' ;;
    *) return 1 ;;
  esac
}

# updater 指向的稳定 asset 名(macOS 是独立 app.tar.gz;Linux 是 AppImage;Windows 是 NSIS setup.exe)。
kxen_release_updater_asset() {
  case "$1" in
    macos-aarch64) printf 'kxen-macos-aarch64.app.tar.gz\n' ;;
    macos-x86_64) printf 'kxen-macos-x86_64.app.tar.gz\n' ;;
    linux-x86_64) printf 'kxen-linux-x86_64.AppImage\n' ;;
    linux-aarch64) printf 'kxen-linux-aarch64.AppImage\n' ;;
    windows-x86_64) printf 'kxen-windows-x86_64-setup.exe\n' ;;
    windows-aarch64) printf 'kxen-windows-aarch64-setup.exe\n' ;;
    *) return 1 ;;
  esac
}

# tauri 产出的 updater 原始文件名(.sig trusted comment 绑定的名字)。
# 稳定 asset 改名后验签仍需对照原始名,由本函数确定性派生。
kxen_release_updater_original_name() {
  local platform="$1"
  local version="$2"
  case "$platform" in
    macos-aarch64 | macos-x86_64) printf 'Kxen.app.tar.gz\n' ;;
    linux-x86_64) printf 'Kxen_%s_amd64.AppImage\n' "$version" ;;
    linux-aarch64) printf 'Kxen_%s_aarch64.AppImage\n' "$version" ;;
    windows-x86_64) printf 'Kxen_%s_x64-setup.exe\n' "$version" ;;
    windows-aarch64) printf 'Kxen_%s_arm64-setup.exe\n' "$version" ;;
    *) return 1 ;;
  esac
}

kxen_release_web_asset() {
  case "$1" in
    windows-*) printf 'kxen-%s.zip\n' "$1" ;;
    macos-* | linux-*) printf 'kxen-%s.tar.gz\n' "$1" ;;
    *) return 1 ;;
  esac
}

# tauri bundle 产物 -> 稳定 asset 名,每行 `bundle 子目录|find pattern|目标名`。
kxen_release_asset_map() {
  case "$1" in
    macos-aarch64)
      printf 'dmg|Kxen_*_aarch64.dmg|kxen-macos-aarch64.dmg\n'
      printf 'macos|Kxen.app.tar.gz|kxen-macos-aarch64.app.tar.gz\n'
      printf 'macos|Kxen.app.tar.gz.sig|kxen-macos-aarch64.app.tar.gz.sig\n'
      ;;
    macos-x86_64)
      printf 'dmg|Kxen_*_x64.dmg|kxen-macos-x86_64.dmg\n'
      printf 'macos|Kxen.app.tar.gz|kxen-macos-x86_64.app.tar.gz\n'
      printf 'macos|Kxen.app.tar.gz.sig|kxen-macos-x86_64.app.tar.gz.sig\n'
      ;;
    linux-x86_64)
      printf 'appimage|Kxen_*_amd64.AppImage|kxen-linux-x86_64.AppImage\n'
      printf 'appimage|Kxen_*_amd64.AppImage.sig|kxen-linux-x86_64.AppImage.sig\n'
      printf 'deb|Kxen_*_amd64.deb|kxen-linux-x86_64.deb\n'
      ;;
    linux-aarch64)
      printf 'appimage|Kxen_*_aarch64.AppImage|kxen-linux-aarch64.AppImage\n'
      printf 'appimage|Kxen_*_aarch64.AppImage.sig|kxen-linux-aarch64.AppImage.sig\n'
      printf 'deb|Kxen_*_arm64.deb|kxen-linux-aarch64.deb\n'
      ;;
    windows-x86_64)
      printf 'nsis|Kxen_*_x64-setup.exe|kxen-windows-x86_64-setup.exe\n'
      printf 'nsis|Kxen_*_x64-setup.exe.sig|kxen-windows-x86_64-setup.exe.sig\n'
      ;;
    windows-aarch64)
      printf 'nsis|Kxen_*_arm64-setup.exe|kxen-windows-aarch64-setup.exe\n'
      printf 'nsis|Kxen_*_arm64-setup.exe.sig|kxen-windows-aarch64-setup.exe.sig\n'
      ;;
    *) return 1 ;;
  esac
}

# 一个平台的全部稳定 asset 名(桌面 bundle + kxen 包),每行一个。
kxen_release_assets() {
  local platform="$1"
  kxen_release_asset_map "$platform" | cut -d'|' -f3
  kxen_release_web_asset "$platform"
}

kxen_release_manifest_json() {
  local platform
  local entries=()
  for platform in "${KXEN_RELEASE_PLATFORMS[@]}"; do
    entries+=("$(
      jq -cn \
        --arg platform "$platform" \
        --arg runner "$(kxen_release_runner "$platform")" \
        --arg target "$(kxen_release_target "$platform")" \
        --argjson signed "$(kxen_release_signed "$platform")" \
        '{ platform: $platform, runner: $runner, target: $target, signed: $signed }'
    )")
  done
  printf '%s\n' "${entries[@]}" | jq -s '{ include: . }'
}

kxen_release_manifest_main() {
  local command="${1:-}"
  local platform="${2:-}"
  case "$command" in
    json) kxen_release_manifest_json ;;
    platforms) printf '%s\n' "${KXEN_RELEASE_PLATFORMS[@]}" ;;
    runner | target | os | signed | updater-key | updater-asset | web-asset)
      kxen_release_platform_exists "$platform" || return 1
      "kxen_release_${command//-/_}" "$platform"
      ;;
    updater-original)
      kxen_release_platform_exists "$platform" || return 1
      kxen_release_updater_original_name "$platform" "${3:-}"
      ;;
    assets)
      kxen_release_platform_exists "$platform" || return 1
      kxen_release_assets "$platform"
      ;;
    *)
      printf 'usage: release-manifest.sh <json|platforms|runner|target|os|signed|updater-key|updater-asset|updater-original|web-asset|assets> [platform] [version]\n' >&2
      return 1
      ;;
  esac
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  set -euo pipefail
  kxen_release_manifest_main "$@"
fi
