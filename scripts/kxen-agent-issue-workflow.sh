#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'FAIL %s\n' "$*" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command is unavailable: $1"
}

require_file() {
  [[ -f "$1" ]] || fail "required file is unavailable: $1"
}

sha256_stream() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum | awk '{ print $1 }'
  else
    shasum -a 256 | awk '{ print $1 }'
  fi
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{ print $1 }'
  else
    shasum -a 256 "$1" | awk '{ print $1 }'
  fi
}

extract_result() {
  local events="$1"
  local output="$2"
  local temporary="${output}.tmp.$$"
  require_command jq
  require_file "$events"
  umask 077
  jq -s -e '
    [.[] | select(.type == "run_finished")][-1].result.finalText
    | select(type == "string" and length > 0)
    | fromjson
    | select(type == "object")
  ' "$events" > "$temporary" || {
    rm -f "$temporary"
    fail "DCPRun did not produce one valid JSON result: $events"
  }
  chmod 0600 "$temporary"
  mv "$temporary" "$output"
}

validate_result() {
  local role="$1"
  local result="$2"
  require_command jq
  require_file "$result"
  case "$role" in
    context)
      jq -e '
        .status == "PASS" and
        (.issue | type == "object") and
        (.taskContract | type == "object") and
        has("security")
      ' "$result" >/dev/null
      ;;
    fixer)
      jq -e '
        .status == "PASS" and
        (.rootCause | type == "string" and length > 0) and
        (.changedFiles | type == "array" and all(.[]; type == "string" and length > 0)) and
        (.checks | type == "array") and
        (.security | type == "object")
      ' "$result" >/dev/null
      ;;
    reviewer)
      jq -e '
        .status == "PASS" and
        (.changedFiles | type == "array" and all(.[]; type == "string" and length > 0)) and
        (.findings | type == "array" and length == 0) and
        (.checks | type == "array") and
        (.security | type == "object")
      ' "$result" >/dev/null
      ;;
    *) fail "unknown result role: $role" ;;
  esac || fail "$role result is not a valid PASS verdict: $result"
  printf 'PASS validated %s result: %s\n' "$role" "$result" >&2
}

source_fingerprint() {
  require_command git
  {
    git rev-parse --verify HEAD
    git diff --binary --no-ext-diff HEAD --
    while IFS= read -r path; do
      [[ -n "$path" ]] || continue
      printf 'untracked %s %s\n' "$(sha256_file "$path")" "$path"
    done < <(git ls-files --others --exclude-standard | LC_ALL=C sort)
  } | sha256_stream
}

candidate_files_json() {
  local base="$1"
  local temporary_index="${TMPDIR:-/tmp}/kxen-agent-files-index.$$"
  require_command git
  require_command jq
  git rev-parse --verify "${base}^{commit}" >/dev/null 2>&1 || fail "invalid diff base: $base"
  rm -f "$temporary_index"
  GIT_INDEX_FILE="$temporary_index" git read-tree "$base"
  GIT_INDEX_FILE="$temporary_index" git add -A
  GIT_INDEX_FILE="$temporary_index" git diff --cached --name-only -z "$base" -- \
    | jq -Rs 'split("\u0000") | map(select(length > 0)) | sort'
  rm -f "$temporary_index"
}

validate_diff() {
  local base="$1"
  local temporary_index="${TMPDIR:-/tmp}/kxen-agent-index.$$"
  local files=()
  local path
  local record
  local additions
  local deletions
  local changed_path
  local changed_lines=0
  require_command git
  git rev-parse --verify "${base}^{commit}" >/dev/null 2>&1 || fail "invalid diff base: $base"
  rm -f "$temporary_index"
  GIT_INDEX_FILE="$temporary_index" git read-tree "$base"
  GIT_INDEX_FILE="$temporary_index" git add -A
  while IFS= read -r -d '' path; do
    files+=("$path")
  done < <(GIT_INDEX_FILE="$temporary_index" git diff --cached --name-only -z "$base" --)
  if [[ "${#files[@]}" -eq 0 ]]; then
    rm -f "$temporary_index"
    fail "candidate diff is empty"
  fi
  if [[ "${#files[@]}" -gt 20 ]]; then
    rm -f "$temporary_index"
    fail "candidate diff exceeds 20 files: ${#files[@]}"
  fi
  for path in "${files[@]}"; do
    case "$path" in
      .gitattributes | .gitmodules | .lfsconfig | .agents/kxen/* | .github/CODEOWNERS | .github/actions/* | .github/workflows/* | \
        examples/kxen-agent/github-issue/* | scripts/kxen-agent-issue-workflow.sh)
        rm -f "$temporary_index"
        fail "candidate diff changes an automation trust boundary: $path"
        ;;
    esac
    if [[ "$path" == *$'\n'* || "$path" == *$'\r'* || "$path" == *$'\t'* ]]; then
      rm -f "$temporary_index"
      fail "candidate path contains control characters"
    fi
  done
  while IFS= read -r -d '' record; do
    IFS=$'\t' read -r additions deletions changed_path <<< "$record"
    if [[ "$additions" == "-" || "$deletions" == "-" ]]; then
      rm -f "$temporary_index"
      fail "candidate diff contains a binary file: $changed_path"
    fi
    changed_lines=$((changed_lines + additions + deletions))
  done < <(GIT_INDEX_FILE="$temporary_index" git diff --cached --numstat -z "$base" --)
  if [[ "$changed_lines" -gt 1200 ]]; then
    rm -f "$temporary_index"
    fail "candidate diff exceeds 1200 changed lines: $changed_lines"
  fi
  GIT_INDEX_FILE="$temporary_index" git diff --cached --check "$base" --
  rm -f "$temporary_index"
  printf 'PASS validated candidate diff: %s files, %s changed lines\n' "${#files[@]}" "$changed_lines" >&2
}

authorization_header() {
  [[ -n "${GH_TOKEN:-}" ]] || fail "GH_TOKEN is required"
  printf 'x-access-token:%s' "$GH_TOKEN" | base64 | tr -d '\r\n'
}

repository_url() {
  [[ "${GITHUB_REPOSITORY:-}" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || fail "GITHUB_REPOSITORY is invalid"
  printf 'https://github.com/%s.git' "$GITHUB_REPOSITORY"
}

prepare_branch() {
  local branch="$1"
  local base="$2"
  local header
  local remote_url
  require_command git
  git check-ref-format --branch "$branch" >/dev/null 2>&1 || fail "invalid topic branch: $branch"
  git check-ref-format --branch "$base" >/dev/null 2>&1 || fail "invalid base branch: $base"
  [[ -z "$(git status --porcelain)" ]] || fail "Workspace must be clean before branch preparation"
  header="AUTHORIZATION: basic $(authorization_header)"
  remote_url="$(repository_url)"
  GIT_TERMINAL_PROMPT=0 git -c "http.https://github.com/.extraheader=$header" fetch --no-tags "$remote_url" \
    "+refs/heads/$base:refs/remotes/origin/$base"
  if GIT_TERMINAL_PROMPT=0 git -c "http.https://github.com/.extraheader=$header" ls-remote --exit-code --heads "$remote_url" \
    "refs/heads/$branch" >/dev/null 2>&1; then
    GIT_TERMINAL_PROMPT=0 git -c "http.https://github.com/.extraheader=$header" fetch --no-tags "$remote_url" \
      "+refs/heads/$branch:refs/remotes/origin/$branch"
    git checkout -B "$branch" "origin/$branch"
  else
    git checkout -B "$branch" "origin/$base"
  fi
  printf 'PASS prepared branch: %s\n' "$branch"
}

export_patch() {
  local base="$1"
  local output="$2"
  local temporary_index="${TMPDIR:-/tmp}/kxen-agent-export-index.$$"
  local temporary_output="${output}.tmp.$$"
  require_command git
  git rev-parse --verify "${base}^{commit}" >/dev/null 2>&1 || fail "invalid patch base: $base"
  rm -f "$temporary_index" "$temporary_output"
  GIT_INDEX_FILE="$temporary_index" git read-tree "$base"
  GIT_INDEX_FILE="$temporary_index" git add -A
  if GIT_INDEX_FILE="$temporary_index" git diff --cached --quiet "$base" --; then
    rm -f "$temporary_index"
    fail "candidate patch is empty"
  fi
  umask 077
  GIT_INDEX_FILE="$temporary_index" git diff --cached --binary --full-index "$base" -- > "$temporary_output"
  rm -f "$temporary_index"
  chmod 0600 "$temporary_output"
  mv "$temporary_output" "$output"
  printf 'PASS exported candidate patch: %s\n' "$output" >&2
}

apply_patch_file() {
  local patch="$1"
  require_command git
  require_file "$patch"
  [[ -s "$patch" ]] || fail "candidate patch is empty: $patch"
  [[ -z "$(git status --porcelain)" ]] || fail "Workspace must be clean before applying the candidate patch"
  git apply --check --whitespace=error-all "$patch"
  git apply --whitespace=error-all "$patch"
  printf 'PASS applied candidate patch: %s\n' "$patch" >&2
}

prepare_publish_branch() {
  local branch="$1"
  local base="$2"
  local expected_parent="$3"
  local header
  local remote_url
  local actual_parent
  require_command git
  git check-ref-format --branch "$branch" >/dev/null 2>&1 || fail "invalid topic branch: $branch"
  git check-ref-format --branch "$base" >/dev/null 2>&1 || fail "invalid base branch: $base"
  [[ "$expected_parent" =~ ^[0-9a-f]{40}$ ]] || fail "invalid candidate parent commit: $expected_parent"
  [[ -z "$(git status --porcelain)" ]] || fail "Workspace must be clean before publisher branch preparation"
  header="AUTHORIZATION: basic $(authorization_header)"
  remote_url="$(repository_url)"
  GIT_TERMINAL_PROMPT=0 git -c "http.https://github.com/.extraheader=$header" fetch --no-tags "$remote_url" \
    "+refs/heads/$base:refs/remotes/origin/$base"
  if GIT_TERMINAL_PROMPT=0 git -c "http.https://github.com/.extraheader=$header" ls-remote --exit-code --heads "$remote_url" \
    "refs/heads/$branch" >/dev/null 2>&1; then
    GIT_TERMINAL_PROMPT=0 git -c "http.https://github.com/.extraheader=$header" fetch --no-tags "$remote_url" \
      "+refs/heads/$branch:refs/remotes/origin/$branch"
    actual_parent="$(git rev-parse "origin/$branch")"
    [[ "$actual_parent" == "$expected_parent" ]] || fail "topic branch changed after candidate generation"
    git checkout -B "$branch" "origin/$branch"
  else
    actual_parent="$(git rev-parse "origin/$base")"
    [[ "$actual_parent" == "$expected_parent" ]] || fail "base branch changed after candidate generation"
    git checkout -B "$branch" "origin/$base"
  fi
  printf 'PASS prepared isolated publisher branch: %s\n' "$branch" >&2
}

publish_fix() {
  local issue_number="$1"
  local branch="$2"
  local base="$3"
  local fixer_result="$4"
  local reviewer_result="$5"
  local header
  local remote_url
  local pr_url
  local checks
  local changed_files
  [[ "$issue_number" =~ ^[1-9][0-9]*$ ]] || fail "invalid issue number: $issue_number"
  git check-ref-format --branch "$branch" >/dev/null 2>&1 || fail "invalid topic branch: $branch"
  git check-ref-format --branch "$base" >/dev/null 2>&1 || fail "invalid base branch: $base"
  validate_result fixer "$fixer_result"
  validate_result reviewer "$reviewer_result"
  validate_diff "origin/$base"
  changed_files="$(candidate_files_json "origin/$base")"
  jq -e --argjson expected "$changed_files" '(.changedFiles | sort | unique) == $expected' "$fixer_result" >/dev/null \
    || fail "fixer changedFiles does not match the candidate diff"
  jq -e --argjson expected "$changed_files" '(.changedFiles | sort | unique) == $expected' "$reviewer_result" >/dev/null \
    || fail "reviewer changedFiles does not match the candidate diff"
  [[ "$(git symbolic-ref --short HEAD)" == "$branch" ]] || fail "publisher is not on the expected topic branch"
  if [[ "${KXEN_AGENT_PUBLISH_DRY_RUN:-0}" == "1" ]]; then
    jq -n \
      --argjson issue "$issue_number" \
      --arg branch "$branch" \
      --arg base "$base" \
      '{status:"PASS", mode:"dry-run", issue:$issue, branch:$branch, base:$base}'
    return
  fi
  require_command gh
  [[ -n "${GH_TOKEN:-}" ]] || fail "GH_TOKEN is required"
  git add -A
  git diff --cached --check
  git config user.name 'github-actions[bot]'
  git config user.email '41898282+github-actions[bot]@users.noreply.github.com'
  git -c core.hooksPath=/dev/null -c core.fsmonitor=false commit -m "fix: resolve issue #$issue_number"
  header="AUTHORIZATION: basic $(authorization_header)"
  remote_url="$(repository_url)"
  GIT_TERMINAL_PROMPT=0 git -c "http.https://github.com/.extraheader=$header" push "$remote_url" "HEAD:refs/heads/$branch"
  pr_url="$(gh pr list --repo "$GITHUB_REPOSITORY" --state open --head "$branch" --json url --jq '.[0].url // empty')"
  checks="$(jq -c '.checks' "$reviewer_result")"
  if [[ -z "$pr_url" ]]; then
    pr_url="$(gh pr create \
      --repo "$GITHUB_REPOSITORY" \
      --draft \
      --base "$base" \
      --head "$branch" \
      --title "fix: resolve issue #$issue_number" \
      --body "Automated candidate fix produced by the isolated kxen-agent verifier, fixer, and reviewer pipeline.\n\nFixes #$issue_number\n\nIndependent reviewer checks: \`$checks\`\n\nThis draft requires normal repository review and CI before merge.")"
  fi
  gh issue comment "$issue_number" --repo "$GITHUB_REPOSITORY" --body "kxen-agent produced a reviewed draft fix: $pr_url"
  jq -n --arg status PASS --arg prUrl "$pr_url" --arg branch "$branch" '{status:$status, prUrl:$prUrl, branch:$branch}'
}

usage() {
  printf '%s\n' \
    'usage:' \
    '  kxen-agent-issue-workflow.sh extract-result EVENTS_JSONL OUTPUT_JSON' \
    '  kxen-agent-issue-workflow.sh validate-result context|fixer|reviewer RESULT_JSON' \
    '  kxen-agent-issue-workflow.sh source-fingerprint' \
    '  kxen-agent-issue-workflow.sh validate-diff BASE_REF' \
    '  kxen-agent-issue-workflow.sh prepare-branch BRANCH BASE_BRANCH' \
    '  kxen-agent-issue-workflow.sh export-patch BASE_REF OUTPUT_PATCH' \
    '  kxen-agent-issue-workflow.sh apply-patch PATCH' \
    '  kxen-agent-issue-workflow.sh prepare-publish-branch BRANCH BASE_BRANCH EXPECTED_PARENT' \
    '  kxen-agent-issue-workflow.sh publish ISSUE BRANCH BASE_BRANCH FIXER_JSON REVIEWER_JSON'
}

command="${1:-}"
case "$command" in
  extract-result)
    [[ "$#" -eq 3 ]] || { usage >&2; exit 2; }
    extract_result "$2" "$3"
    ;;
  validate-result)
    [[ "$#" -eq 3 ]] || { usage >&2; exit 2; }
    validate_result "$2" "$3"
    ;;
  source-fingerprint)
    [[ "$#" -eq 1 ]] || { usage >&2; exit 2; }
    source_fingerprint
    ;;
  validate-diff)
    [[ "$#" -eq 2 ]] || { usage >&2; exit 2; }
    validate_diff "$2"
    ;;
  prepare-branch)
    [[ "$#" -eq 3 ]] || { usage >&2; exit 2; }
    prepare_branch "$2" "$3"
    ;;
  export-patch)
    [[ "$#" -eq 3 ]] || { usage >&2; exit 2; }
    export_patch "$2" "$3"
    ;;
  apply-patch)
    [[ "$#" -eq 2 ]] || { usage >&2; exit 2; }
    apply_patch_file "$2"
    ;;
  prepare-publish-branch)
    [[ "$#" -eq 4 ]] || { usage >&2; exit 2; }
    prepare_publish_branch "$2" "$3" "$4"
    ;;
  publish)
    [[ "$#" -eq 6 ]] || { usage >&2; exit 2; }
    publish_fix "$2" "$3" "$4" "$5" "$6"
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
