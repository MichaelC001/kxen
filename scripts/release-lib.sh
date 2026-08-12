#!/usr/bin/env bash

_kxen_release_lib_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# 平台矩阵与稳定 asset 命名单一出处,合并 latest.json 与产物核对都依赖它。
# shellcheck source=scripts/release-manifest.sh
source "$_kxen_release_lib_dir/release-manifest.sh"
unset _kxen_release_lib_dir

kxen_require_release_tag() {
  local release_tag="$1"
  local pattern='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
  if [[ ! "$release_tag" =~ $pattern ]]; then
    printf 'invalid stable release tag: %s\n' "$release_tag"
    return 1
  fi
}

kxen_require_github_repository() {
  local repository="$1"
  if [[ ! "$repository" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
    printf 'invalid GitHub repository: %s\n' "$repository"
    return 1
  fi
}

kxen_compare_decimal_release_components() {
  local left="$1"
  local right="$2"
  local LC_ALL=C
  if [[ "${#left}" -gt "${#right}" ]]; then
    printf '1\n'
  elif [[ "${#left}" -lt "${#right}" ]]; then
    printf '%s\n' '-1'
  elif [[ "$left" == "$right" ]]; then
    printf '0\n'
  elif [[ "$left" > "$right" ]]; then
    printf '1\n'
  else
    printf '%s\n' '-1'
  fi
}

kxen_compare_stable_release_tags() {
  local left="$1"
  local right="$2"
  local left_major left_minor left_patch
  local right_major right_minor right_patch
  local comparison
  kxen_require_release_tag "$left" >/dev/null || return 1
  kxen_require_release_tag "$right" >/dev/null || return 1
  IFS=. read -r left_major left_minor left_patch <<< "${left#v}"
  IFS=. read -r right_major right_minor right_patch <<< "${right#v}"
  for comparison in \
    "$(kxen_compare_decimal_release_components "$left_major" "$right_major")" \
    "$(kxen_compare_decimal_release_components "$left_minor" "$right_minor")" \
    "$(kxen_compare_decimal_release_components "$left_patch" "$right_patch")"; do
    if [[ "$comparison" != 0 ]]; then
      printf '%s\n' "$comparison"
      return 0
    fi
  done
  printf '0\n'
}

kxen_latest_published_stable_tag_from_json() {
  local requested_tag="$1"
  local stable_pattern='^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
  local tags
  local tag
  local latest=''
  local comparison
  kxen_require_release_tag "$requested_tag" >/dev/null || return 1
  tags="$({
    jq -r \
      --arg requested "$requested_tag" \
      --arg stable_pattern "$stable_pattern" \
      '
        if type != "array" or any(.[]; type != "array") then
          error("GitHub releases response must be an array of page arrays")
        else
          [
            .[][] |
            if type != "object"
              or (.tag_name | type) != "string"
              or (.draft | type) != "boolean"
              or (.prerelease | type) != "boolean"
            then
              error("GitHub release entry has invalid tag_name, draft, or prerelease fields")
            elif .draft == false and .prerelease == false and .tag_name != $requested then
              if (.tag_name | test($stable_pattern)) then
                .tag_name
              else
                error("invalid published stable release tag: \(.tag_name)")
              end
            else
              empty
            end
          ] | .[]
        end
      '
  } 2>&1)" || {
    printf '%s\n' "$tags" >&2
    return 1
  }
  while IFS= read -r tag; do
    [[ -n "$tag" ]] || continue
    if [[ -z "$latest" ]]; then
      latest="$tag"
      continue
    fi
    comparison="$(kxen_compare_stable_release_tags "$tag" "$latest")" || return 1
    if [[ "$comparison" == 1 ]]; then
      latest="$tag"
    fi
  done <<< "$tags"
  if [[ -n "$latest" ]]; then
    printf '%s\n' "$latest"
  fi
}

kxen_require_release_above_published_stable() {
  local release_tag="$1"
  local repository="$2"
  local pages
  local latest
  local comparison
  kxen_require_release_tag "$release_tag" || return 1
  kxen_require_github_repository "$repository" || return 1
  if ! pages="$(gh api --paginate "repos/$repository/releases?per_page=100" --slurp)"; then
    printf 'unable to list published releases for %s\n' "$repository" >&2
    return 1
  fi
  if ! latest="$(printf '%s\n' "$pages" | kxen_latest_published_stable_tag_from_json "$release_tag")"; then
    printf 'unable to determine the published stable release baseline for %s\n' "$repository" >&2
    return 1
  fi
  if [[ -z "$latest" ]]; then
    printf 'PASS no prior published stable release exists for %s\n' "$repository"
    return 0
  fi
  comparison="$(kxen_compare_stable_release_tags "$release_tag" "$latest")" || return 1
  if [[ "$comparison" != 1 ]]; then
    printf 'release tag %s must be strictly newer than published stable release %s\n' "$release_tag" "$latest" >&2
    return 1
  fi
  printf 'PASS release tag %s is newer than published stable release %s\n' "$release_tag" "$latest"
}

kxen_validate_release_notes_file() {
  local release_tag="$1"
  local release_notes_path="$2"
  local version
  local version_regex
  kxen_require_release_tag "$release_tag" >/dev/null || return 1
  kxen_require_regular_file_size 'release notes' "$release_notes_path" 131072 || return 1
  version="${release_tag#v}"
  version_regex="${version//./\\.}"
  if [[ "$(grep -Ec "^## \[$version_regex\]( - [0-9]{4}-[0-9]{2}-[0-9]{2})?$" "$release_notes_path" || true)" != 1 ]]; then
    printf 'release notes must contain exactly one version heading for %s\n' "$version" >&2
    return 1
  fi
  if [[ "$(grep -Ec '^## \[[^]]+\]' "$release_notes_path" || true)" != 1 ]]; then
    printf 'release notes must describe exactly one version: %s\n' "$release_tag" >&2
    return 1
  fi
  if ! grep -Eq '^- .+' "$release_notes_path"; then
    printf 'release notes do not contain any change entries: %s\n' "$release_notes_path" >&2
    return 1
  fi
}

kxen_render_release_body() {
  local release_tag="$1"
  local repository="$2"
  local release_notes_path="$3"
  local workflow_marker="${4:-}"
  local distribution_profile="${5:-six-platform}"
  kxen_require_release_tag "$release_tag" >/dev/null || return 1
  kxen_require_github_repository "$repository" >/dev/null || return 1
  kxen_validate_release_notes_file "$release_tag" "$release_notes_path" || return 1

  cat "$release_notes_path"
  printf '\n完整变更记录见 [CHANGELOG.md](https://github.com/%s/blob/main/CHANGELOG.md)。\n\n' \
    "$repository"
  printf '## 下载与安装\n\n'
  if [[ "$distribution_profile" == macos-aarch64-only ]]; then
    printf '%s\n' \
      '- 桌面版: macOS 14+ Apple Silicon，Developer ID 签名并经 Apple 公证。' \
      '- 自动更新: `Kxen.app.tar.gz` 与签名文件。' \
      '- 手动安装: 下载 DMG，打开后将 Kxen 拖入 Applications。'
  elif [[ "$distribution_profile" == six-platform ]]; then
    printf '%s\n' \
      '- 桌面版: macOS Apple Silicon/Intel、Windows x64/ARM64、Linux x86_64/ARM64。' \
      '- macOS: DMG 与 updater archive 均经 Developer ID 签名和 Apple 公证。' \
      '- Windows: 提供 NSIS installer；当前未做 Authenticode 签名，首次启动可能出现 SmartScreen reputation warning，可选择 `More info` -> `Run anyway`。' \
      '- Linux: 提供 AppImage 与 deb。' \
      '- Headless server: `kxen-<platform>.tar.gz`，Windows 为 `.zip`。运行 `kxen` 后会打印带 token 的访问 URL；远程访问建议使用 `tailscale serve`，并通过 `--allow-host` 放行 tailnet hostname。' \
      '- 完整性: 使用 `SHA256SUMS` 校验下载文件；`latest.json` 和对应 `.sig` 供自动更新使用。'
  else
    printf 'unknown release distribution profile: %s\n' "$distribution_profile" >&2
    return 1
  fi
  if [[ -n "$workflow_marker" ]]; then
    printf '\n%s\n' "$workflow_marker"
  fi
}

kxen_find_one() {
  local label="$1"
  local directory="$2"
  local pattern="$3"
  local matches=()
  while IFS= read -r path; do
    matches+=("$path")
  done < <(find "$directory" -maxdepth 1 -type f -name "$pattern" -print | sort)
  if [[ "${#matches[@]}" -ne 1 ]]; then
    printf 'expected exactly one %s below %s, found %s\n' "$label" "$directory" "${#matches[@]}" >&2
    return 1
  fi
  printf '%s\n' "${matches[0]}"
}

kxen_require_regular_file_size() {
  local label="$1"
  local path="$2"
  local maximum_bytes="$3"
  python3 - "$label" "$path" "$maximum_bytes" <<'PY'
import os
import stat
import sys

label, path, maximum_bytes_raw = sys.argv[1:]
maximum_bytes = int(maximum_bytes_raw)
try:
    metadata = os.lstat(path)
except OSError as error:
    raise SystemExit(f"{label} cannot be inspected: {path}: {error}") from error
if not stat.S_ISREG(metadata.st_mode):
    raise SystemExit(f"{label} must be a regular file: {path}")
if metadata.st_size <= 0:
    raise SystemExit(f"{label} is empty: {path}")
if metadata.st_size > maximum_bytes:
    raise SystemExit(f"{label} exceeds {maximum_bytes} bytes: {path}")
PY
}

kxen_verify_macos_updater_archive() {
  local archive_path="$1"
  local expected_version="${2:-}"
  local expected_identifier="${3:-}"
  python3 - "$archive_path" "$expected_version" "$expected_identifier" <<'PY'
import posixpath
from pathlib import PurePosixPath
import plistlib
import os
import stat
import sys
import tarfile

archive_path = sys.argv[1]
expected_version = sys.argv[2]
expected_identifier = sys.argv[3]
archive_stat = os.lstat(archive_path)
if not stat.S_ISREG(archive_stat.st_mode):
    raise SystemExit("updater archive must be a regular file")
if archive_stat.st_size > 512 * 1024 * 1024:
    raise SystemExit("updater archive exceeds 512 MiB")
info_plist_data = None
has_executable = False
seen_names = set()
member_count = 0
total_size = 0
with tarfile.open(archive_path, mode="r:gz") as archive:
    for member in archive:
        member_count += 1
        if member_count > 100_000:
            raise SystemExit("updater archive contains more than 100000 entries")
        if member.size < 0:
            raise SystemExit(f"updater archive entry has a negative size: {member.name}")
        total_size += member.size
        if total_size > 2 * 1024 * 1024 * 1024:
            raise SystemExit("updater archive expands beyond 2 GiB")
        name = member.name
        canonical_name = PurePosixPath(name).as_posix()
        comparable_name = name.rstrip("/") if member.isdir() else name
        if "\\" in name or comparable_name != canonical_name:
            raise SystemExit(f"non-canonical updater archive entry: {name}")
        if canonical_name in seen_names:
            raise SystemExit(f"duplicate updater archive entry: {name}")
        seen_names.add(canonical_name)
        parts = PurePosixPath(name).parts
        if name.startswith("/") or ".." in parts or not parts or parts[0] != "Kxen.app":
            raise SystemExit(f"unsafe updater archive entry: {name}")
        if not (member.isfile() or member.isdir() or member.issym() or member.islnk()):
            raise SystemExit(f"unsupported updater archive entry type: {name}")
        if member.issym() or member.islnk():
            if canonical_name in {
                "Kxen.app",
                "Kxen.app/Contents",
                "Kxen.app/Contents/MacOS",
            }:
                raise SystemExit(f"critical updater archive directory cannot be a link: {name}")
            target = member.linkname
            if target.startswith("/") or "\\" in target:
                raise SystemExit(f"unsafe updater archive link: {name} -> {target}")
            if member.issym():
                target = posixpath.join(posixpath.dirname(name), target)
            resolved = posixpath.normpath(target)
            if resolved != "Kxen.app" and not resolved.startswith("Kxen.app/"):
                raise SystemExit(f"unsafe updater archive link: {name} -> {member.linkname}")
        if member.isfile() and name == "Kxen.app/Contents/Info.plist":
            info_file = archive.extractfile(member)
            if info_file is None:
                raise SystemExit("unable to read updater Info.plist")
            info_plist_data = info_file.read(1_048_577)
            if len(info_plist_data) > 1_048_576:
                raise SystemExit("updater Info.plist exceeds 1 MiB")
        if (
            member.isfile()
            and name.startswith("Kxen.app/Contents/MacOS/")
            and member.mode & 0o111
        ):
            has_executable = True
if member_count == 0:
    raise SystemExit("updater archive is empty")
if info_plist_data is None:
    raise SystemExit("updater archive does not contain Kxen.app/Contents/Info.plist")
if not has_executable:
    raise SystemExit("updater archive does not contain an executable under Kxen.app/Contents/MacOS")
try:
    info = plistlib.loads(info_plist_data)
except Exception as error:
    raise SystemExit(f"updater Info.plist is invalid: {error}") from error
if info.get("CFBundlePackageType") != "APPL":
    raise SystemExit("updater Info.plist is not an application bundle")
if expected_version and info.get("CFBundleShortVersionString") != expected_version:
    raise SystemExit("updater Info.plist version does not match the release")
if expected_identifier and info.get("CFBundleIdentifier") != expected_identifier:
    raise SystemExit("updater Info.plist identifier does not match the application")
PY
}

kxen_verify_updater_signature() {
  local archive_path="$1"
  local signature_path="$2"
  # 产物改名后 trusted comment 仍绑定 tauri 原始文件名,由 release-manifest.sh 派生传入;
  # 缺省按 archive basename 校验(未改名场景)。
  local original_name="${3:-}"
  local tauri_config="${4:-src-tauri/tauri.conf.json}"
  local public_key
  public_key="$(jq -er '.plugins.updater.pubkey | select(type == "string" and length > 0)' "$tauri_config")"
  local library_dir
  library_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  if [[ -n "$original_name" ]]; then
    node "$library_dir/verify-updater-signature.mjs" "$archive_path" "$signature_path" "$public_key" "$original_name"
  else
    node "$library_dir/verify-updater-signature.mjs" "$archive_path" "$signature_path" "$public_key"
  fi
}

# sha256 摘要跨平台封装:Linux/Windows(Git Bash)用 sha256sum,macOS 用 shasum。
kxen_sha256sum() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$@"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$@"
  else
    printf 'sha256sum or shasum is required\n' >&2
    return 1
  fi
}

kxen_sha256sum_check() {
  local manifest_path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -c "$manifest_path"
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c "$manifest_path"
  else
    printf 'sha256sum or shasum is required\n' >&2
    return 1
  fi
}

# 为目录内全部常规文件(不含 SHA256SUMS 自身)生成校验清单。
kxen_write_sha256sums() {
  local dir="$1"
  (
    cd "$dir"
    local names=()
    local name
    while IFS= read -r name; do
      names+=("$name")
    done < <(find . -maxdepth 1 -type f ! -name SHA256SUMS -print | sed 's|^\./||' | LC_ALL=C sort)
    if [[ "${#names[@]}" -eq 0 ]]; then
      printf 'no files to checksum in %s\n' "$dir" >&2
      return 1
    fi
    kxen_sha256sum "${names[@]}" > SHA256SUMS
  )
}

# 合并各平台 updater 签名生成 latest.json(tauri updater 格式)。
# 无 .sig 的平台跳过并告警(updater 对该平台不可用);sig 存在但 archive 缺失、
# sig 为空、或全部平台都无 sig 时失败。
kxen_merge_updater_manifest() {
  local version="$1"
  local repository="$2"
  local release_tag="$3"
  local asset_dir="$4"
  local output_path="$5"
  local release_notes="${6:-}"
  local entries=()
  local platform key asset signature url
  if [[ -z "$release_notes" ]]; then
    printf 'release-specific updater notes are required: %s\n' "$release_tag" >&2
    return 1
  fi
  if [[ "${#release_notes}" -gt 131072 ]]; then
    printf 'updater notes exceed 131072 characters: %s\n' "$release_tag" >&2
    return 1
  fi
  local notes_version_regex
  notes_version_regex="${version//./\\.}"
  if ! grep -Eq "^## \[$notes_version_regex\]( - [0-9]{4}-[0-9]{2}-[0-9]{2})?$" <<< "$release_notes" \
    || ! grep -Eq '^- .+' <<< "$release_notes"; then
    printf 'updater notes are not structured release-specific content: %s\n' "$release_tag" >&2
    return 1
  fi
  for platform in "${KXEN_RELEASE_PLATFORMS[@]}"; do
    key="$(kxen_release_updater_key "$platform")" || return 1
    asset="$(kxen_release_updater_asset "$platform")" || return 1
    if [[ ! -f "$asset_dir/$asset.sig" ]]; then
      printf 'SKIP updater platform without signature: %s (%s.sig)\n' "$key" "$asset" >&2
      continue
    fi
    if [[ ! -f "$asset_dir/$asset" ]]; then
      printf 'updater signature without archive: %s\n' "$asset" >&2
      return 1
    fi
    signature="$(cat "$asset_dir/$asset.sig")"
    if [[ -z "$signature" ]]; then
      printf 'updater signature is empty: %s.sig\n' "$asset" >&2
      return 1
    fi
    url="https://github.com/$repository/releases/download/$release_tag/$asset"
    entries+=("$(
      jq -cn \
        --arg key "$key" \
        --arg signature "$signature" \
        --arg url "$url" \
        '{ key: $key, entry: { signature: $signature, url: $url } }'
    )")
  done
  if [[ "${#entries[@]}" -eq 0 ]]; then
    printf 'no signed updater platform found in %s\n' "$asset_dir" >&2
    return 1
  fi
  local pub_date
  pub_date="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  printf '%s\n' "${entries[@]}" | jq -s \
    --arg version "$version" \
    --arg notes "$release_notes" \
    --arg pub_date "$pub_date" \
    '{
      version: $version,
      notes: $notes,
      pub_date: $pub_date,
      platforms: (map({ (.key): .entry }) | add)
    }' > "$output_path"
}
