#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"
operation="${1:-}"
release_tag="${2:-}"
output_path="${3:-$repo_root/CHANGELOG.md}"
git_cliff_version='2.13.1'

usage() {
  printf 'usage: changelog.sh <generate|check|release-notes> <tag> [output-path]\n' >&2
  exit 1
}

[[ "$operation" == generate || "$operation" == check || "$operation" == release-notes ]] || usage

# shellcheck source=scripts/release-lib.sh
source "$script_dir/release-lib.sh"
kxen_require_release_tag "$release_tag"

if ! command -v git-cliff >/dev/null 2>&1; then
  printf 'git-cliff %s is required; install it with: cargo install --locked git-cliff --version %s\n' \
    "$git_cliff_version" "$git_cliff_version" >&2
  exit 1
fi
actual_version="$(git-cliff --version)"
if [[ "$actual_version" != "git-cliff $git_cliff_version" ]]; then
  printf 'git-cliff version mismatch: expected %s, got %s\n' \
    "$git_cliff_version" "$actual_version" >&2
  exit 1
fi

cd "$repo_root"
if git show-ref --verify --quiet "refs/tags/$release_tag"; then
  tag_args=()
else
  tag_args=(--tag "$release_tag")
fi

release_range=''
if [[ "$operation" == release-notes ]]; then
  if git show-ref --verify --quiet "refs/tags/$release_tag"; then
    seen_target=false
    previous_tag=''
    while IFS= read -r candidate; do
      if [[ "$seen_target" == true ]]; then
        previous_tag="$candidate"
        break
      fi
      if [[ "$candidate" == "$release_tag" ]]; then
        seen_target=true
      fi
    done < <(git tag --merged "$release_tag" --sort=-version:refname | grep -E '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$')
    if [[ "$seen_target" != true ]]; then
      printf 'release tag is not reachable in local history: %s\n' "$release_tag" >&2
      exit 1
    fi
    if [[ -n "$previous_tag" ]]; then
      release_range="$previous_tag..$release_tag"
    fi
  else
    previous_tag="$(git tag --merged HEAD --sort=-version:refname | grep -E '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$' | head -1)"
    if [[ -n "$previous_tag" ]]; then
      release_range="$previous_tag..HEAD"
    fi
  fi
fi

generate() {
  local destination="$1"
  local strip_args=()
  local range_args=()
  if [[ "$operation" == release-notes ]]; then
    strip_args=(--strip header)
    if [[ -n "$release_range" ]]; then
      range_args=("$release_range")
    else
      strip_args=(--current --strip header --tag-pattern "^${release_tag//./\\.}$")
    fi
  fi
  git-cliff \
    --config "$repo_root/cliff.toml" \
    "${tag_args[@]}" \
    "${strip_args[@]}" \
    --output "$destination" \
    "${range_args[@]}"
  python3 - "$destination" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
content = path.read_text(encoding="utf-8").rstrip("\n") + "\n"
path.write_text(content, encoding="utf-8")
PY
}

if [[ "$operation" == check ]]; then
  generated="$(mktemp "${TMPDIR:-/tmp}/kxen-changelog.XXXXXX")"
  trap 'rm -f "$generated"' EXIT
  generate "$generated"
  if ! cmp -s "$generated" "$output_path"; then
    printf 'CHANGELOG.md is stale; run: scripts/changelog.sh generate %s\n' "$release_tag" >&2
    diff -u "$output_path" "$generated" || true
    exit 1
  fi
  printf 'PASS generated changelog matches %s\n' "$release_tag"
else
  generate "$output_path"
  printf 'PASS generated %s for %s: %s\n' "$operation" "$release_tag" "$output_path"
fi
