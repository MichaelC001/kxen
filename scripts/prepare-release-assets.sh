#!/usr/bin/env bash
set -euo pipefail

# 用法: prepare-release-assets.sh <platform> <tag> <owner/repo> <bundle_root> [output_dir]
# platform 取值见 scripts/release-manifest.sh(平台/命名单一出处)。
# 从 tauri bundle 目录收集桌面产物并按稳定名改名,分别打包 kxen 与 kxen-agent,输出到 output_dir。
# latest.json 与 SHA256SUMS 不在此生成:由 publish 段合并全平台后统一产出。

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/release-lib.sh
source "$script_dir/release-lib.sh"

platform="${1:-}"
release_tag="${2:-}"
repository="${3:-}"
bundle_root="${4:-}"
output_dir="${5:-release-assets}"

kxen_release_platform_exists "$platform"
kxen_require_release_tag "$release_tag"
kxen_require_github_repository "$repository"
if [[ ! -d "$bundle_root" ]]; then
  printf 'bundle directory not found: %s\n' "$bundle_root"
  exit 1
fi
version="${release_tag#v}"

mkdir -p "$output_dir"
if [[ -n "$(find "$output_dir" -mindepth 1 -maxdepth 1 -print -quit)" ]]; then
  printf 'release asset directory must be empty: %s\n' "$output_dir"
  exit 1
fi

# 桌面 bundle 产物:按 release-manifest.sh 的映射收集并改名为稳定 asset 名。
while IFS='|' read -r subdir pattern dest; do
  [[ -n "$subdir" ]] || continue
  src_path="$(kxen_find_one "$dest" "$bundle_root/$subdir" "$pattern")"
  src_name="$(basename "$src_path")"
  # 带版本号的 tauri 产物(Kxen_<version>_*)必须与 release tag 一致;
  # 不带版本号的固定名产物(Kxen.app.tar.gz 及其 .sig)由后续内容与签名校验覆盖。
  if [[ "$pattern" == 'Kxen_*'* && "$src_name" != "Kxen_${version}_"* ]]; then
    printf 'bundle name does not match release version %s: %s\n' "$version" "$src_name"
    exit 1
  fi
  if [[ ! "$dest" =~ ^[A-Za-z0-9._+-]+$ ]]; then
    printf 'release asset name requires URL encoding and is refused: %s\n' "$dest"
    exit 1
  fi
  kxen_require_regular_file_size "$dest" "$src_path" 2147483648
  cp -p "$src_path" "$output_dir/$dest"
done < <(kxen_release_asset_map "$platform")

# updater 条目:macOS 校验 app.tar.gz 结构,全平台校验 minisign 签名非空且可验证。
updater_asset="$(kxen_release_updater_asset "$platform")"
updater_path="$output_dir/$updater_asset"
signature_path="$updater_path.sig"
if [[ "$(kxen_release_os "$platform")" == macos ]]; then
  kxen_require_regular_file_size 'updater archive' "$updater_path" 536870912
  app_identifier="$(jq -er '.identifier | select(type == "string" and length > 0)' src-tauri/tauri.conf.json)"
  kxen_verify_macos_updater_archive "$updater_path" "$version" "$app_identifier"
fi
kxen_require_regular_file_size 'updater signature' "$signature_path" 65536
if [[ -z "$(cat "$signature_path")" ]]; then
  printf 'updater signature is empty: %s\n' "$signature_path"
  exit 1
fi
updater_original="$(kxen_release_updater_original_name "$platform" "$version")"
kxen_verify_updater_signature "$updater_path" "$signature_path" "$updater_original"

# kxen 无头 server 与 kxen-agent 独立 CLI 使用并列的 tar.gz(unix)/zip(windows) asset。
target="$(kxen_release_target "$platform")"
web_asset="$(kxen_release_web_asset "$platform")"
agent_asset="$(kxen_release_agent_asset "$platform")"
web_dir="target/$target/release"
web_binary="kxen"
agent_binary="kxen-agent"
if [[ "$(kxen_release_os "$platform")" == windows ]]; then
  web_binary="kxen.exe"
  agent_binary="kxen-agent.exe"
fi
for binary in "$web_binary" "$agent_binary"; do
  if [[ ! -f "$web_dir/$binary" ]]; then
    printf 'headless binary not found: %s\n' "$web_dir/$binary"
    exit 1
  fi
done
# macOS 的两个 CLI 必须已 Developer ID 签名(release.yml 在构建腿签名并公证);未签名即失败,不打包。
if [[ "$(kxen_release_os "$platform")" == macos ]]; then
  codesign --verify --deep --strict --verbose=2 "$web_dir/$web_binary"
  codesign --verify --deep --strict --verbose=2 "$web_dir/$agent_binary"
fi
web_out="$(cd "$output_dir" && pwd)/$web_asset"
agent_out="$(cd "$output_dir" && pwd)/$agent_asset"
if [[ "$web_asset" == *.zip ]]; then
  (cd "$web_dir" && 7z a -tzip "$web_out" "$web_binary" >/dev/null)
  (cd "$web_dir" && 7z a -tzip "$agent_out" "$agent_binary" >/dev/null)
else
  tar -czf "$web_out" -C "$web_dir" "$web_binary"
  tar -czf "$agent_out" -C "$web_dir" "$agent_binary"
fi
kxen_require_regular_file_size "$web_asset" "$web_out" 536870912
kxen_require_regular_file_size "$agent_asset" "$agent_out" 536870912

bash "$script_dir/verify-release-assets.sh" "$release_tag" "$repository" "$output_dir" "$platform"
printf 'PASS prepared release assets: %s (%s)\n' "$output_dir" "$platform"
