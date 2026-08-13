#!/usr/bin/env bash
set -euo pipefail

# 用法: verify-release-assets.sh <tag> <owner/repo> [asset_dir] [platform...]
# 带 platform 参数:核对该平台的产物子集(prepare-release-assets.sh 逐平台调用)。
# 不带 platform 参数:核对全平台完整产物集,含 latest.json 与 SHA256SUMS(publish 段调用)。
# 期望产物清单全部派生自 scripts/release-manifest.sh。

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/release-lib.sh
source "$script_dir/release-lib.sh"

release_tag="${1:-}"
repository="${2:-}"
asset_dir="${3:-release-assets}"
platforms=("${@:4}")

kxen_require_release_tag "$release_tag"
kxen_require_github_repository "$repository"
if [[ ! -d "$asset_dir" ]]; then
  printf 'release asset directory not found: %s\n' "$asset_dir"
  exit 1
fi

full_set=0
if [[ "${#platforms[@]}" -eq 0 ]]; then
  full_set=1
  platforms=("${KXEN_RELEASE_PLATFORMS[@]}")
fi
for platform in "${platforms[@]}"; do
  kxen_release_platform_exists "$platform"
done
version="${release_tag#v}"

# 期望文件集 == manifest 派生的稳定 asset 名。
# latest.json 与 SHA256SUMS 由 publish 段(或本地发布)在全平台合并后生成:
# 完整集模式必然要求;子集模式下只要存在就纳入核对(local-release.sh 单平台路径)。
check_manifests="$full_set"
if [[ -f "$asset_dir/latest.json" || -f "$asset_dir/SHA256SUMS" ]]; then
  check_manifests=1
fi
expected=()
for platform in "${platforms[@]}"; do
  while IFS= read -r name; do
    expected+=("$name")
  done < <(kxen_release_assets "$platform")
done
if [[ "$check_manifests" == 1 ]]; then
  expected+=(latest.json SHA256SUMS)
fi

actual=()
while IFS= read -r path; do
  actual+=("$(basename "$path")")
done < <(find "$asset_dir" -mindepth 1 -maxdepth 1 -type f -print | LC_ALL=C sort)
entry_count="$(find "$asset_dir" -mindepth 1 -maxdepth 1 -print | wc -l | tr -d ' ')"
if [[ "$entry_count" != "${#actual[@]}" ]]; then
  printf 'release asset directory must contain only regular files: %s\n' "$asset_dir"
  exit 1
fi

expected_sorted="$(printf '%s\n' "${expected[@]}" | LC_ALL=C sort)"
actual_sorted="$(printf '%s\n' "${actual[@]}")"
if [[ "$actual_sorted" != "$expected_sorted" ]]; then
  printf 'release asset set mismatch\nexpected:\n%s\nactual:\n%s\n' "$expected_sorted" "$actual_sorted"
  exit 1
fi

for name in "${actual[@]}"; do
  if [[ ! "$name" =~ ^[A-Za-z0-9._+-]+$ ]]; then
    printf 'release asset name requires URL encoding and is refused: %s\n' "$name"
    exit 1
  fi
  kxen_require_regular_file_size 'release asset' "$asset_dir/$name" 2147483648
done

# 逐平台 updater 校验:签名非空且可被 tauri.conf.json 的 updater 公钥验证;
# macOS 额外深校验 app.tar.gz 结构、版本与 identifier。
app_identifier="$(jq -er '.identifier | select(type == "string" and length > 0)' src-tauri/tauri.conf.json)"
for platform in "${platforms[@]}"; do
  updater_asset="$(kxen_release_updater_asset "$platform")"
  updater_path="$asset_dir/$updater_asset"
  signature_path="$updater_path.sig"
  kxen_require_regular_file_size 'updater signature' "$signature_path" 65536
  if [[ -z "$(cat "$signature_path")" ]]; then
    printf 'updater signature is empty: %s\n' "$signature_path"
    exit 1
  fi
  if [[ "$(kxen_release_os "$platform")" == macos ]]; then
    kxen_require_regular_file_size 'updater archive' "$updater_path" 536870912
    kxen_verify_macos_updater_archive "$updater_path" "$version" "$app_identifier"
  fi
  updater_original="$(kxen_release_updater_original_name "$platform" "$version")"
  kxen_verify_updater_signature "$updater_path" "$signature_path" "$updater_original"
  web_asset="$(kxen_release_web_asset "$platform")"
  agent_asset="$(kxen_release_agent_asset "$platform")"
  if [[ "$(kxen_release_os "$platform")" == windows ]]; then
    packaged_server="$(unzip -Z1 "$asset_dir/$web_asset" | sed 's#^\./##' | LC_ALL=C sort)"
    packaged_agent="$(unzip -Z1 "$asset_dir/$agent_asset" | sed 's#^\./##' | LC_ALL=C sort)"
    expected_server='kxen.exe'
    expected_agent='kxen-agent.exe'
  else
    packaged_server="$(tar -tzf "$asset_dir/$web_asset" | sed 's#^\./##' | LC_ALL=C sort)"
    packaged_agent="$(tar -tzf "$asset_dir/$agent_asset" | sed 's#^\./##' | LC_ALL=C sort)"
    expected_server='kxen'
    expected_agent='kxen-agent'
  fi
  if [[ "$packaged_server" != "$expected_server" || "$packaged_agent" != "$expected_agent" ]]; then
    printf 'headless package content mismatch for %s\nexpected server: %s\nactual server: %s\nexpected agent: %s\nactual agent: %s\n' \
      "$platform" "$expected_server" "$packaged_server" "$expected_agent" "$packaged_agent"
    exit 1
  fi
  # macOS 的两个独立 CLI asset 都必须包含 Developer ID 签名的二进制。
  # publish 段在 Linux runner 上做全平台核对,无 codesign 时跳过(release.yml 构建腿与本地路径都会执行)。
  if [[ "$(kxen_release_os "$platform")" == macos ]] && command -v codesign >/dev/null 2>&1; then
    cli_verify_dir="$(mktemp -d "${TMPDIR:-/tmp}/kxen-cli-verify.XXXXXX")"
    tar -xzf "$asset_dir/$web_asset" -C "$cli_verify_dir"
    tar -xzf "$asset_dir/$agent_asset" -C "$cli_verify_dir"
    codesign --verify --deep --strict --verbose=2 "$cli_verify_dir/kxen"
    codesign --verify --deep --strict --verbose=2 "$cli_verify_dir/kxen-agent"
    rm -rf "$cli_verify_dir"
  fi
done

if [[ "$check_manifests" == 1 ]]; then
  checksums_path="$asset_dir/SHA256SUMS"
  latest_path="$asset_dir/latest.json"
  kxen_require_regular_file_size 'checksum manifest' "$checksums_path" 65536
  kxen_require_regular_file_size 'updater manifest' "$latest_path" 1048576

  # SHA256SUMS 精确覆盖其余全部文件且逐条校验通过。
  checksummed="$(awk '{ print $2 }' "$checksums_path" | LC_ALL=C sort)"
  expected_checksummed="$(printf '%s\n' "${actual[@]}" | grep -v '^SHA256SUMS$' | LC_ALL=C sort)"
  if [[ "$checksummed" != "$expected_checksummed" ]]; then
    printf 'checksum manifest coverage mismatch\nexpected:\n%s\nactual:\n%s\n' \
      "$expected_checksummed" "$checksummed"
    exit 1
  fi
  (
    cd "$asset_dir"
    kxen_sha256sum_check SHA256SUMS
  )

  # latest.json:结构合法,platform 条目与 manifest updater key 一一对应,
  # signature 与 url 和实际 asset 一致。
  jq -e \
    --arg version "$version" \
    '
      ((keys | sort) == ["notes", "platforms", "pub_date", "version"]) and
      .version == $version and
      (.notes | type == "string" and length > 0) and
      (.pub_date | type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) and
      (.platforms | type == "object" and length > 0) and
      all(.platforms[];
        ((keys | sort) == ["signature", "url"]) and
        (.signature | type == "string" and length > 0) and
        (.url | type == "string" and startswith("https://github.com/")))
    ' "$latest_path" >/dev/null

  actual_keys="$(jq -r '.platforms | keys[]' "$latest_path" | LC_ALL=C sort)"
  expected_keys=()
  for platform in "${platforms[@]}"; do
    updater_asset="$(kxen_release_updater_asset "$platform")"
    # 合并逻辑会跳过无 sig 平台;核对的目录里 sig 必须全部存在,否则上面的集合核对已失败。
    if [[ -f "$asset_dir/$updater_asset.sig" ]]; then
      expected_keys+=("$(kxen_release_updater_key "$platform")")
    fi
  done
  expected_keys_sorted="$(printf '%s\n' "${expected_keys[@]}" | LC_ALL=C sort)"
  if [[ "$actual_keys" != "$expected_keys_sorted" ]]; then
    printf 'updater manifest platform keys mismatch\nexpected:\n%s\nactual:\n%s\n' \
      "$expected_keys_sorted" "$actual_keys"
    exit 1
  fi
  for platform in "${platforms[@]}"; do
    updater_asset="$(kxen_release_updater_asset "$platform")"
    signature_path="$asset_dir/$updater_asset.sig"
    [[ -f "$signature_path" ]] || continue
    key="$(kxen_release_updater_key "$platform")"
    signature="$(cat "$signature_path")"
    updater_url="https://github.com/$repository/releases/download/$release_tag/$updater_asset"
    jq -e \
      --arg key "$key" \
      --arg signature "$signature" \
      --arg url "$updater_url" \
      '.platforms[$key].signature == $signature and .platforms[$key].url == $url' \
      "$latest_path" >/dev/null
  done
  printf 'PASS updater manifest: %s (%s platforms)\n' "$version" "${#expected_keys[@]}"
fi

printf 'PASS release asset set: %s (%s files)\n' "$asset_dir" "${#actual[@]}"
