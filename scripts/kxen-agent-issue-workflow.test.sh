#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
helper="$script_dir/kxen-agent-issue-workflow.sh"
fixture="$(mktemp -d "${TMPDIR:-/tmp}/kxen-agent-issue-workflow.XXXXXX")"
trap 'rm -rf "$fixture"' EXIT

git -C "$fixture" init -q
git -C "$fixture" config user.name test
git -C "$fixture" config user.email test@example.com
printf 'before\n' > "$fixture/source.txt"
git -C "$fixture" add source.txt
git -C "$fixture" commit -qm 'test: baseline'
git -C "$fixture" branch -M main
git -C "$fixture" update-ref refs/remotes/origin/main HEAD
artifacts="$fixture/.git/test-artifacts"
mkdir -p "$artifacts"

printf '%s\n' \
  '{"type":"session_created","sessionId":"ses_test"}' \
  '{"type":"run_finished","result":{"finalText":"{\"status\":\"PASS\",\"rootCause\":\"fixture\",\"changedFiles\":[\"source.txt\"],\"checks\":[],\"security\":{}}"}}' \
  > "$artifacts/fixer.jsonl"
"$helper" extract-result "$artifacts/fixer.jsonl" "$artifacts/fixer.json"
"$helper" validate-result fixer "$artifacts/fixer.json"

printf '%s\n' '{"status":"PASS","findings":[],"changedFiles":["source.txt"],"checks":["fixture"],"security":{}}' \
  > "$artifacts/reviewer.json"
"$helper" validate-result reviewer "$artifacts/reviewer.json"

printf 'after\n' > "$fixture/source.txt"
(
  cd "$fixture"
  before_fingerprint="$("$helper" source-fingerprint)"
  after_fingerprint="$("$helper" source-fingerprint)"
  [[ "$before_fingerprint" == "$after_fingerprint" ]]
  "$helper" validate-diff refs/remotes/origin/main
  "$helper" export-patch HEAD "$artifacts/candidate.patch"
  printf 'before\n' > source.txt
  "$helper" apply-patch "$artifacts/candidate.patch"
  git checkout -qb kxen-agent/issue-7
  KXEN_AGENT_PUBLISH_DRY_RUN=1 "$helper" publish 7 kxen-agent/issue-7 main \
    "$artifacts/fixer.json" "$artifacts/reviewer.json" \
    | jq -e '.status == "PASS" and .mode == "dry-run" and .issue == 7' >/dev/null
  [[ "$(git rev-parse HEAD)" == "$(git rev-parse refs/remotes/origin/main)" ]]
)

printf '%s\n' '{"status":"PASS","findings":[],"changedFiles":["other.txt"],"checks":["fixture"],"security":{}}' \
  > "$artifacts/reviewer-mismatch.json"
if (
  cd "$fixture" && KXEN_AGENT_PUBLISH_DRY_RUN=1 "$helper" publish 7 kxen-agent/issue-7 main \
    "$artifacts/fixer.json" "$artifacts/reviewer-mismatch.json" >/dev/null 2>&1
); then
  printf 'FAIL reviewer changedFiles mismatch was accepted\n' >&2
  exit 1
fi

mkdir -p "$fixture/.github/workflows"
printf 'name: forbidden\n' > "$fixture/.github/workflows/change.yml"
if (cd "$fixture" && "$helper" validate-diff refs/remotes/origin/main >/dev/null 2>&1); then
  printf 'FAIL protected workflow change was accepted\n' >&2
  exit 1
fi
rm -f "$fixture/.github/workflows/change.yml"

printf '%s\n' '{"status":"FAIL","findings":[],"changedFiles":[],"checks":[],"security":{}}' > "$artifacts/reviewer-fail.json"
if "$helper" validate-result reviewer "$artifacts/reviewer-fail.json" >/dev/null 2>&1; then
  printf 'FAIL reviewer FAIL verdict was accepted\n' >&2
  exit 1
fi

workflow="$script_dir/../.github/workflows/kxen-issue-autofix.yml"
[[ -f "$workflow" ]]
[[ "$(grep -cF 'environment: agent-automation' "$workflow")" -eq 3 ]]
[[ "$(grep -cF 'image: ubuntu:24.04@sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea' "$workflow")" -eq 3 ]]
[[ "$(grep -cF 'options: --security-opt no-new-privileges:true --cap-drop=SYS_PTRACE' "$workflow")" -eq 3 ]]
[[ "$(grep -cF -- '--consume-auth-file' "$workflow")" -eq 3 ]]
! grep -qF 'GITHUB_PERSONAL_ACCESS_TOKEN' "$workflow"
! grep -qF 'github-mcp-server' "$workflow"
awk '
  /^      - name:/ { step = $0 }
  /XAI_API_KEY: \$\{\{ secrets\.XAI_API_KEY \}\}/ {
    if (step !~ /Materialize one-shot Provider credential/) exit 1
    count += 1
  }
  END { if (count != 3) exit 1 }
' "$workflow"
awk '
  /^      - name:/ { step = $0 }
  /GH_TOKEN: \$\{\{ github\.token \}\}/ {
    if (step ~ /Run context verifier|Run repository fixer|Run independent reviewer/) exit 1
  }
' "$workflow"
publisher="$(sed -n '/^  publish:/,/^  report-failure:/p' "$workflow")"
[[ "$publisher" != *'environment: agent-automation'* ]]
[[ "$publisher" != *'XAI_API_KEY'* ]]
[[ "$publisher" == *'cp "$GITHUB_WORKSPACE/scripts/kxen-agent-issue-workflow.sh" "$trusted/issue-workflow.sh"'* ]]
[[ "$publisher" == *'helper="$trusted/issue-workflow.sh"'* ]]

printf 'PASS kxen-agent GitHub Issue workflow helpers\n'
