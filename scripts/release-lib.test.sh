#!/usr/bin/env bash
set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/release-lib.sh
source "$script_dir/release-lib.sh"

failures=0

fail() {
  printf 'FAIL %s\n' "$1" >&2
  failures=$((failures + 1))
}

assert_compare() {
  local expected="$1"
  local left="$2"
  local right="$3"
  local actual
  if ! actual="$(kxen_compare_stable_release_tags "$left" "$right" 2>/dev/null)"; then
    fail "compare $left to $right returned an error"
  elif [[ "$actual" != "$expected" ]]; then
    fail "compare $left to $right expected $expected, got $actual"
  fi
}

assert_json_max() {
  local expected="$1"
  local requested="$2"
  local releases_json="$3"
  local actual
  if ! actual="$(printf '%s\n' "$releases_json" | kxen_latest_published_stable_tag_from_json "$requested" 2>/dev/null)"; then
    fail "JSON maximum for $requested returned an error"
  elif [[ "$actual" != "$expected" ]]; then
    fail "JSON maximum for $requested expected ${expected:-<empty>}, got ${actual:-<empty>}"
  fi
}

assert_json_rejected() {
  local label="$1"
  local requested="$2"
  local releases_json="$3"
  if printf '%s\n' "$releases_json" | kxen_latest_published_stable_tag_from_json "$requested" >/dev/null 2>&1; then
    fail "$label was accepted"
  fi
}

assert_ci_url() {
  local expected="$1"
  local commit="$2"
  local workflow_runs_json="$3"
  local actual
  if ! actual="$(printf '%s\n' "$workflow_runs_json" | kxen_successful_main_ci_url_from_json "$commit" 2>/dev/null)"; then
    fail "successful CI evidence for $commit returned an error"
  elif [[ "$actual" != "$expected" ]]; then
    fail "successful CI evidence for $commit expected $expected, got ${actual:-<empty>}"
  fi
}

assert_ci_rejected() {
  local label="$1"
  local commit="$2"
  local workflow_runs_json="$3"
  if printf '%s\n' "$workflow_runs_json" | kxen_successful_main_ci_url_from_json "$commit" >/dev/null 2>&1; then
    fail "$label was accepted as successful main CI evidence"
  fi
}

assert_compare 0 v1.2.3 v1.2.3
assert_compare 1 v2.0.0 v1.999.999
assert_compare 1 v1.10.0 v1.9.999
assert_compare 1 v1.0.1 v1.0.0
assert_compare -1 v0.9.9 v1.0.0
assert_compare 1 v184467440737095516160.0.0 v184467440737095516159.999.999
if kxen_compare_stable_release_tags v01.2.3 v1.2.3 >/dev/null 2>&1; then
  fail 'invalid comparison operand was accepted'
fi

commit_a='0123456789abcdef0123456789abcdef01234567'
commit_b='89abcdef0123456789abcdef0123456789abcdef'
if ! kxen_require_release_source_parity \
  v1.2.3 "$commit_a" origin/main "$commit_a" workflow "$commit_a" >/dev/null; then
  fail 'exact release source parity was rejected'
fi
if kxen_require_release_source_parity \
  v1.2.3 "$commit_a" origin/main "$commit_b" workflow "$commit_a" >/dev/null 2>&1; then
  fail 'tag and main source mismatch was accepted'
fi
if kxen_require_release_source_parity \
  v1.2.3 "$commit_a" origin/main "$commit_a" workflow "$commit_b" >/dev/null 2>&1; then
  fail 'tag and workflow source mismatch was accepted'
fi
if kxen_require_release_source_parity \
  v1.2.3 invalid origin/main "$commit_a" workflow "$commit_a" >/dev/null 2>&1; then
  fail 'invalid release source commit was accepted'
fi

ci_runs="$(jq -cn \
  --arg commit "$commit_a" \
  '{workflow_runs:[
    {
      head_sha:$commit,
      head_branch:"main",
      event:"push",
      status:"completed",
      conclusion:"success",
      run_attempt:1,
      run_number:40,
      html_url:"https://github.com/example/project/actions/runs/40"
    },
    {
      head_sha:$commit,
      head_branch:"main",
      event:"push",
      status:"completed",
      conclusion:"success",
      run_attempt:2,
      run_number:41,
      html_url:"https://github.com/example/project/actions/runs/41"
    }
  ]}')"
assert_ci_url 'https://github.com/example/project/actions/runs/41' "$commit_a" "$ci_runs"
assert_ci_rejected 'failed CI run' "$commit_a" "$(printf '%s\n' "$ci_runs" | jq '.workflow_runs |= map(.conclusion = "failure")')"
assert_ci_rejected 'pull request CI run' "$commit_a" "$(printf '%s\n' "$ci_runs" | jq '.workflow_runs |= map(.event = "pull_request")')"
assert_ci_rejected 'different commit CI run' "$commit_b" "$ci_runs"
assert_ci_rejected 'malformed CI response' "$commit_a" '{"workflow_runs":{}}'
assert_ci_rejected 'untrusted CI URL' "$commit_a" "$(printf '%s\n' "$ci_runs" | jq '.workflow_runs |= map(.html_url = "https://example.com/actions/runs/41")')"

releases='[
  [
    {"tag_name":"v1.2.3","draft":false,"prerelease":false},
    {"tag_name":"v99.0.0","draft":true,"prerelease":false},
    {"tag_name":"v100.0.0","draft":false,"prerelease":true}
  ],
  [
    {"tag_name":"v1.10.0","draft":false,"prerelease":false},
    {"tag_name":"v2.0.0","draft":false,"prerelease":false}
  ]
]'
assert_json_max v1.10.0 v2.0.0 "$releases"
assert_json_max '' v1.0.0 '[[
  {"tag_name":"v1.0.0","draft":false,"prerelease":false},
  {"tag_name":"v9.0.0","draft":true,"prerelease":false},
  {"tag_name":"v10.0.0","draft":false,"prerelease":true}
]]'
assert_json_rejected 'invalid published stable tag' v2.0.0 '[[{"tag_name":"v01.2.3","draft":false,"prerelease":false}]]'
assert_json_rejected 'malformed releases envelope' v2.0.0 '{"tag_name":"v1.2.3","draft":false,"prerelease":false}'
assert_json_rejected 'malformed release object' v2.0.0 '[[{"tag_name":"v1.2.3","prerelease":false}]]'

mock_gh_mode='success'
mock_releases="$releases"
gh() {
  if [[ "$mock_gh_mode" == failure ]]; then
    return 1
  fi
  printf '%s\n' "$mock_releases"
}
mock_releases="$ci_runs"
if ! kxen_require_successful_main_ci example/project "$commit_a" >/dev/null 2>&1; then
  fail 'successful API-backed exact-commit CI gate was rejected'
fi
if kxen_require_successful_main_ci example/project "$commit_b" >/dev/null 2>&1; then
  fail 'API-backed CI gate accepted a different commit'
fi
mock_gh_mode='failure'
if kxen_require_successful_main_ci example/project "$commit_a" >/dev/null 2>&1; then
  fail 'GitHub workflow runs API failure was accepted'
fi
mock_gh_mode='success'
mock_releases="$releases"
if ! kxen_require_release_above_published_stable v2.0.0 example/project >/dev/null 2>&1; then
  fail 'newer release was rejected by the API-backed gate'
fi
if kxen_require_release_above_published_stable v1.5.0 example/project >/dev/null 2>&1; then
  fail 'release below the published maximum was accepted'
fi
mock_releases='[[]]'
if ! kxen_require_release_above_published_stable v1.0.0 example/project >/dev/null 2>&1; then
  fail 'first stable release was rejected'
fi
mock_gh_mode='failure'
if kxen_require_release_above_published_stable v2.0.0 example/project >/dev/null 2>&1; then
  fail 'GitHub API failure was accepted'
fi

# Release notes:只接受 git-cliff 生成的单版本内容，再追加分发说明。
notes_dir="$(mktemp -d "${TMPDIR:-/tmp}/kxen-release-notes-test.XXXXXX")"
cat > "$notes_dir/v1.2.0.md" <<'EOF'
## [1.2.0]

> **版本主题:** Bot 自动化与可靠协作

### 本次更新

- 每个 Bot 都能通过自己的 Builder 会话持续完善定义。 ([1234567](https://github.com/example/project/commit/1234567890))
- Bot-to-Bot 会话支持可靠恢复。 ([abcdef0](https://github.com/example/project/commit/abcdef0123))

### 新增功能

- **bot:** Add release-specific capability ([1234567](https://github.com/example/project/commit/1234567890))

### 问题修复

- Fix release-specific failure ([abcdef0](https://github.com/example/project/commit/abcdef0123))
EOF
if ! kxen_validate_release_notes_file v1.2.0 "$notes_dir/v1.2.0.md"; then
  fail 'valid generated release notes were rejected'
fi
if [[ "$(kxen_release_title_from_notes v1.2.0 "$notes_dir/v1.2.0.md")" != 'Kxen v1.2.0: Bot 自动化与可靠协作' ]]; then
  fail 'release title was not derived from the generated version theme'
fi
rendered_notes="$(kxen_render_release_body v1.2.0 example/project "$notes_dir/v1.2.0.md" '<!-- workflow-marker -->')" || fail 'release body renderer returned an error'
for expected in '## [1.2.0]' '版本主题' '每个 Bot 都能通过自己的 Builder' 'Add release-specific capability' '## 下载与安装' 'SHA256SUMS' '<!-- workflow-marker -->'; do
  if [[ "$rendered_notes" != *"$expected"* ]]; then
    fail "release body renderer omitted $expected"
  fi
done
if kxen_render_release_body v1.2.0 example/project "$notes_dir/v1.2.0.md" '' invalid-profile >/dev/null 2>&1; then
  fail 'release body renderer accepted an unknown distribution profile'
fi
printf '## [1.2.0]\n\n### 新增功能\n' > "$notes_dir/no-entry.md"
if kxen_validate_release_notes_file v1.2.0 "$notes_dir/no-entry.md" >/dev/null 2>&1; then
  fail 'release notes without entries were accepted'
fi
printf '## [1.2.0]\n\n- one\n\n## [1.1.0]\n\n- two\n' > "$notes_dir/multiple.md"
if kxen_validate_release_notes_file v1.2.0 "$notes_dir/multiple.md" >/dev/null 2>&1; then
  fail 'release notes containing multiple versions were accepted'
fi
printf 'Kxen v1.2.0 development preview.\n' > "$notes_dir/generic.md"
if kxen_validate_release_notes_file v1.2.0 "$notes_dir/generic.md" >/dev/null 2>&1; then
  fail 'retired generic release notes were accepted'
fi
sed '/^> \*\*版本主题:\*\*/d' "$notes_dir/v1.2.0.md" > "$notes_dir/no-theme.md"
if kxen_validate_release_notes_file v1.2.0 "$notes_dir/no-theme.md" >/dev/null 2>&1; then
  fail 'release notes without a version theme were accepted'
fi
awk '/^### 本次更新$/ { skip = 1; next } skip && /^### / { skip = 0 } !skip { print }' \
  "$notes_dir/v1.2.0.md" > "$notes_dir/no-summary.md"
if kxen_validate_release_notes_file v1.2.0 "$notes_dir/no-summary.md" >/dev/null 2>&1; then
  fail 'release notes without a product summary were accepted'
fi

# kxen_merge_updater_manifest fixture:多平台 sig 合并、无 sig 平台跳过、异常输入拒绝。
merge_dir="$(mktemp -d "${TMPDIR:-/tmp}/kxen-release-lib-test.XXXXXX")"
trap 'rm -rf "$merge_dir"' EXIT

make_updater_fixtures() {
  local dir="$1"
  local platform asset
  for platform in "${KXEN_RELEASE_PLATFORMS[@]}"; do
    asset="$(kxen_release_updater_asset "$platform")"
    printf 'archive-%s\n' "$platform" > "$dir/$asset"
    printf 'sig-%s\n' "$platform" > "$dir/$asset.sig"
  done
}

assert_manifest_platform() {
  local label="$1"
  local manifest="$2"
  local platform="$3"
  local key asset
  key="$(kxen_release_updater_key "$platform")"
  asset="$(kxen_release_updater_asset "$platform")"
  if ! jq -e \
    --arg key "$key" \
    --arg signature "sig-$platform" \
    --arg url "https://github.com/example/project/releases/download/v9.9.9/$asset" \
    '.platforms[$key].signature == $signature and .platforms[$key].url == $url' \
    "$manifest" >/dev/null; then
    fail "$label: platform entry mismatch for $key"
  fi
}

make_updater_fixtures "$merge_dir"
updater_notes=$'## [9.9.9]\n\n### 新增功能\n\n- Release-specific updater notes.'
if ! kxen_merge_updater_manifest 9.9.9 example/project v9.9.9 "$merge_dir" "$merge_dir/latest.json" "$updater_notes" 2>/dev/null; then
  fail 'multi-platform merge returned an error'
else
  if [[ "$(jq -r '.platforms | keys | length' "$merge_dir/latest.json")" != 6 ]]; then
    fail 'multi-platform merge did not produce 6 platform entries'
  fi
  if ! jq -e \
    '((keys | sort) == ["notes", "platforms", "pub_date", "version"]) and
     .version == "9.9.9" and
     .notes == "## [9.9.9]\n\n### 新增功能\n\n- Release-specific updater notes." and
     (.pub_date | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$"))' \
    "$merge_dir/latest.json" >/dev/null; then
    fail 'merged manifest envelope is invalid'
  fi
  for platform in "${KXEN_RELEASE_PLATFORMS[@]}"; do
    assert_manifest_platform 'multi-platform merge' "$merge_dir/latest.json" "$platform"
  done
fi

# 无 sig 平台跳过:删除一个 sig 后该 key 消失,其余保留。
skip_asset="$(kxen_release_updater_asset windows-aarch64)"
rm "$merge_dir/$skip_asset.sig"
if ! kxen_merge_updater_manifest 9.9.9 example/project v9.9.9 "$merge_dir" "$merge_dir/latest-skip.json" "$updater_notes" 2>/dev/null; then
  fail 'merge with a missing signature returned an error'
else
  if [[ "$(jq -r '.platforms | keys | length' "$merge_dir/latest-skip.json")" != 5 ]]; then
    fail 'merge with a missing signature did not skip exactly one platform'
  fi
  if jq -e '.platforms["windows-aarch64"]' "$merge_dir/latest-skip.json" >/dev/null; then
    fail 'unsigned platform was not skipped'
  fi
  key="$(kxen_release_updater_key macos-aarch64)"
  asset="$(kxen_release_updater_asset macos-aarch64)"
  if ! jq -e \
    --arg key "$key" \
    --arg signature 'sig-macos-aarch64' \
    --arg url "https://github.com/example/project/releases/download/v9.9.9/$asset" \
    '.platforms[$key].signature == $signature and .platforms[$key].url == $url' \
    "$merge_dir/latest-skip.json" >/dev/null; then
    fail "merge with a missing signature: platform entry mismatch for $key"
  fi
fi

# sig 存在但 archive 缺失必须失败。
rm "$merge_dir/$skip_asset"
printf 'sig-windows-aarch64\n' > "$merge_dir/$skip_asset.sig"
if kxen_merge_updater_manifest 9.9.9 example/project v9.9.9 "$merge_dir" "$merge_dir/latest-orphan.json" "$updater_notes" >/dev/null 2>&1; then
  fail 'signature without archive was accepted'
fi
printf 'archive-windows-aarch64\n' > "$merge_dir/$skip_asset"

# 空 sig 必须失败。
empty_sig_asset="$(kxen_release_updater_asset linux-x86_64)"
: > "$merge_dir/$empty_sig_asset.sig"
if kxen_merge_updater_manifest 9.9.9 example/project v9.9.9 "$merge_dir" "$merge_dir/latest-empty.json" "$updater_notes" >/dev/null 2>&1; then
  fail 'empty signature was accepted'
fi
printf 'sig-linux-x86_64\n' > "$merge_dir/$empty_sig_asset.sig"

# 全部平台无 sig 必须失败。
# merge_dir 子目录,EXIT trap 统一清理。
empty_dir="$(mktemp -d "$merge_dir/empty.XXXXXX")"
if kxen_merge_updater_manifest 9.9.9 example/project v9.9.9 "$empty_dir" "$empty_dir/latest.json" "$updater_notes" >/dev/null 2>&1; then
  fail 'merge without any signature was accepted'
fi
if kxen_merge_updater_manifest 9.9.9 example/project v9.9.9 "$empty_dir" "$empty_dir/latest-no-notes.json" '' >/dev/null 2>&1; then
  fail 'merge without release-specific updater notes was accepted'
fi

# kxen_write_sha256sums:覆盖全部文件且校验通过。
sums_dir="$(mktemp -d "$merge_dir/sums.XXXXXX")"
printf 'a\n' > "$sums_dir/a.txt"
printf 'b\n' > "$sums_dir/b.txt"
if ! kxen_write_sha256sums "$sums_dir"; then
  fail 'writing SHA256SUMS returned an error'
elif [[ "$(wc -l < "$sums_dir/SHA256SUMS" | tr -d ' ')" != 2 ]]; then
  fail 'SHA256SUMS does not cover exactly the two files'
elif ! (cd "$sums_dir" && kxen_sha256sum_check SHA256SUMS >/dev/null 2>&1); then
  fail 'SHA256SUMS verification failed'
fi

if [[ "$failures" -ne 0 ]]; then
  printf '%s release library test(s) failed\n' "$failures" >&2
  exit 1
fi
printf 'PASS release library SemVer and JSON tests\n'
