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

printf 'PASS kxen-agent GitHub Issue workflow helpers\n'
