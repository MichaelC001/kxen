#!/usr/bin/env bash
set -uo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_dir="$(cd "$script_dir/.." && pwd)"
# shellcheck source=website/public/install.sh
source "$repo_dir/website/public/install.sh"
# release-manifest.sh 已作为独立 CI 输入检查，动态仓库路径只在测试运行时解析。
# shellcheck disable=SC1091
source "$script_dir/release-manifest.sh"

failures=0

fail() {
  printf 'FAIL %s\n' "$1" >&2
  failures=$((failures + 1))
}

assert_equal() {
  local label="$1"
  local expected="$2"
  local actual="$3"
  if [[ "$actual" != "$expected" ]]; then
    fail "$label expected '$expected', got '$actual'"
  fi
}

assert_file_text() {
  local label="$1"
  local expected="$2"
  local path="$3"
  local actual
  if [[ ! -f "$path" ]]; then
    fail "$label is missing: $path"
    return
  fi
  actual="$(sed -n '1p' "$path")"
  assert_equal "$label" "$expected" "$actual"
}

assert_equal 'normalize plain version' v1.2.3 "$(kxen_install_normalize_tag 1.2.3)"
assert_equal 'normalize prefixed version' v1.2.3 "$(kxen_install_normalize_tag v1.2.3)"
for invalid_version in 1.2 v01.2.3 1.2.3-beta latest ''; do
  if kxen_install_normalize_tag "$invalid_version" >/dev/null 2>&1; then
    fail "invalid version was accepted: ${invalid_version:-<empty>}"
  fi
done
assert_equal 'x86 architecture' x86_64 "$(kxen_install_map_arch amd64)"
assert_equal 'arm architecture' aarch64 "$(kxen_install_map_arch arm64)"
if kxen_install_map_arch armv7 >/dev/null 2>&1; then
  fail 'armv7 architecture was accepted'
fi
expected_path_line="export PATH='/tmp/kxen dir':\"\$PATH\""
assert_equal 'quoted PATH line' "$expected_path_line" "$(kxen_install_shell_path_line '/tmp/kxen dir')"

for platform in macos-aarch64 macos-x86_64 linux-x86_64 linux-aarch64; do
  os="${platform%%-*}"
  arch="${platform#*-}"
  assert_equal \
    "$platform server asset" \
    "$(kxen_release_web_asset "$platform")" \
    "$(kxen_install_asset_name server "$os" "$arch")"
  assert_equal \
    "$platform agent asset" \
    "$(kxen_release_agent_asset "$platform")" \
    "$(kxen_install_asset_name agent "$os" "$arch")"
done

test_dir="$(mktemp -d "${TMPDIR:-/tmp}/kxen-installers-test.XXXXXX")"
trap 'rm -rf "$test_dir"' EXIT
fixture_dir="$test_dir/fixture"
mock_bin="$test_dir/mock-bin"
mkdir -p "$fixture_dir/server" "$fixture_dir/agent" "$mock_bin"

cat > "$fixture_dir/server/kxen" <<'EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == --help ]]; then
  printf 'fixture kxen help\n'
  exit 0
fi
exit 2
EOF
cat > "$fixture_dir/agent/kxen-agent" <<'EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == --version ]]; then
  printf 'kxen-agent 9.8.7\n'
  exit 0
fi
exit 2
EOF
chmod 0755 "$fixture_dir/server/kxen" "$fixture_dir/agent/kxen-agent"
tar -czf "$fixture_dir/kxen-linux-x86_64.tar.gz" -C "$fixture_dir/server" kxen
tar -czf "$fixture_dir/kxen-agent-linux-x86_64.tar.gz" -C "$fixture_dir/agent" kxen-agent
server_hash="$(kxen_install_actual_checksum "$fixture_dir/kxen-linux-x86_64.tar.gz")"
agent_hash="$(kxen_install_actual_checksum "$fixture_dir/kxen-agent-linux-x86_64.tar.gz")"
installer_hash="$(kxen_install_actual_checksum "$repo_dir/website/public/install.sh")"
printf '%s  %s\n%s  %s\n' \
  "$server_hash" kxen-linux-x86_64.tar.gz \
  "$agent_hash" kxen-agent-linux-x86_64.tar.gz \
  > "$fixture_dir/SHA256SUMS"
printf '%s  install.sh\n' "$installer_hash" > "$fixture_dir/install.sh.sha256"
printf '{"tag_name":"v9.8.7"}\n' > "$fixture_dir/latest-release.json"

cat > "$mock_bin/uname" <<'EOF'
#!/usr/bin/env bash
case "${1:-}" in
  -s) printf 'Linux\n' ;;
  -m) printf 'x86_64\n' ;;
  *) exit 2 ;;
esac
EOF
cat > "$mock_bin/ldd" <<'EOF'
#!/usr/bin/env bash
printf 'ldd (GNU libc) 2.35\n'
EOF
cat > "$mock_bin/curl" <<'EOF'
#!/usr/bin/env bash
output=''
url=''
while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      output="$2"
      shift 2
      ;;
    https://*)
      url="$1"
      shift
      ;;
    *) shift ;;
  esac
done
[[ -n "$output" && -n "$url" ]] || exit 2
case "$url" in
  */install.sh.sha256) source_path="$KXEN_TEST_FIXTURE/install.sh.sha256" ;;
  */releases/latest) source_path="$KXEN_TEST_FIXTURE/latest-release.json" ;;
  */SHA256SUMS) source_path="$KXEN_TEST_FIXTURE/SHA256SUMS" ;;
  */kxen-linux-x86_64.tar.gz) source_path="$KXEN_TEST_FIXTURE/kxen-linux-x86_64.tar.gz" ;;
  */kxen-agent-linux-x86_64.tar.gz) source_path="$KXEN_TEST_FIXTURE/kxen-agent-linux-x86_64.tar.gz" ;;
  *) exit 22 ;;
esac
cp "$source_path" "$output"
if [[ "${KXEN_TEST_CORRUPT_INSTALLER_CHECKSUM:-0}" == 1 && "$url" == */install.sh.sha256 ]]; then
  printf '0%.0s' {1..64} > "$output"
  printf '  install.sh\n' >> "$output"
fi
if [[ "${KXEN_TEST_CORRUPT_AGENT:-0}" == 1 && "$url" == */kxen-agent-linux-x86_64.tar.gz ]]; then
  printf 'corrupt\n' >> "$output"
fi
EOF
chmod 0755 "$mock_bin/uname" "$mock_bin/ldd" "$mock_bin/curl"

self_check_dir="$test_dir/install-self-check"
if PATH="$mock_bin:$PATH" KXEN_TEST_FIXTURE="$fixture_dir" KXEN_TEST_CORRUPT_INSTALLER_CHECKSUM=1 \
  bash "$repo_dir/website/public/install.sh" \
    --version 9.8.7 \
    --install-dir "$self_check_dir" \
    --no-modify-path \
    >/dev/null 2>&1; then
  fail 'installer self-check mismatch was accepted'
elif [[ -e "$self_check_dir/kxen" || -e "$self_check_dir/kxen-agent" ]]; then
  fail 'installer self-check failure changed the installation directory'
fi

all_dir="$test_dir/install all"
if ! PATH="$mock_bin:$PATH" KXEN_TEST_FIXTURE="$fixture_dir" \
  bash "$repo_dir/website/public/install.sh" \
    --version latest \
    --install-dir "$all_dir" \
    --no-modify-path \
    >/dev/null; then
  fail 'offline default all installation failed'
else
  if [[ ! -x "$all_dir/kxen" || ! -x "$all_dir/kxen-agent" ]]; then
    fail 'default all installation did not install both executables'
  fi
fi

agent_dir="$test_dir/install-agent"
if ! PATH="$mock_bin:$PATH" KXEN_TEST_FIXTURE="$fixture_dir" \
  bash "$repo_dir/website/public/install.sh" \
    --agent \
    --version 9.8.7 \
    --install-dir "$agent_dir" \
    --no-modify-path \
    >/dev/null; then
  fail 'offline agent-only installation failed'
elif [[ ! -x "$agent_dir/kxen-agent" || -e "$agent_dir/kxen" ]]; then
  fail 'agent-only installation selected the wrong executables'
fi

rollback_dir="$test_dir/install-rollback"
mkdir -p "$rollback_dir/kxen-agent"
printf 'old-server\n' > "$rollback_dir/kxen"
if PATH="$mock_bin:$PATH" KXEN_TEST_FIXTURE="$fixture_dir" \
  bash "$repo_dir/website/public/install.sh" \
    --version 9.8.7 \
    --install-dir "$rollback_dir" \
    --no-modify-path \
    >/dev/null 2>&1; then
  fail 'directory destination was accepted'
fi
assert_file_text 'preflight failure preserved server' old-server "$rollback_dir/kxen"
if find "$rollback_dir" -maxdepth 1 -name '*.kxen-install.*' -print -quit | grep -q .; then
  fail 'preflight failure left an installer-owned pending file'
fi

checksum_dir="$test_dir/install-checksum"
mkdir -p "$checksum_dir"
printf 'old-server\n' > "$checksum_dir/kxen"
if PATH="$mock_bin:$PATH" KXEN_TEST_FIXTURE="$fixture_dir" KXEN_TEST_CORRUPT_AGENT=1 \
  bash "$repo_dir/website/public/install.sh" \
    --version 9.8.7 \
    --install-dir "$checksum_dir" \
    --no-modify-path \
    >/dev/null 2>&1; then
  fail 'checksum mismatch was accepted'
fi
assert_file_text 'checksum failure preserved server' old-server "$checksum_dir/kxen"

extra_dir="$test_dir/extra"
mkdir -p "$extra_dir"
printf 'one\n' > "$extra_dir/kxen"
printf 'two\n' > "$extra_dir/unexpected"
tar -czf "$test_dir/extra.tar.gz" -C "$extra_dir" kxen unexpected
if (kxen_install_extract_asset "$test_dir/extra.tar.gz" kxen "$test_dir/extra-out") >/dev/null 2>&1; then
  fail 'archive with an extra entry was accepted'
fi

if [[ "$failures" -ne 0 ]]; then
  printf 'FAIL installer tests: %s failure(s)\n' "$failures" >&2
  exit 1
fi
printf 'PASS installer tests\n'
