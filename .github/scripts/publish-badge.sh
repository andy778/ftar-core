#!/usr/bin/env bash
# Publish one shields.io endpoint file to the `badges` branch, which holds only such files.
# The other files there stay, so each workflow can publish its own badge.
#
# Usage: GH_TOKEN=... GITHUB_REPOSITORY=owner/repo publish-badge.sh FILE
# BADGES_REMOTE overrides the repository to push to (to try the script on a local one).
set -euo pipefail

file=$1
name=$(basename "$file")
remote=${BADGES_REMOTE:-"https://x-access-token:${GH_TOKEN}@github.com/${GITHUB_REPOSITORY}.git"}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Another workflow may push between our fetch and our push: start again from its commit.
for attempt in 1 2 3; do
  dir="$work/$attempt"
  git init -q -b badges "$dir"
  # Exit code 2: no such branch yet. Any other failure must not replace the branch.
  code=0
  git ls-remote --exit-code --heads "$remote" refs/heads/badges > /dev/null || code=$?
  case "$code" in
    0) git -C "$dir" pull -q "$remote" badges ;;
    2) ;;
    *) exit "$code" ;;
  esac
  cp "$file" "$dir/$name"
  git -C "$dir" add "$name"
  if git -C "$dir" diff --cached --quiet; then
    echo "$name is unchanged"
    exit 0
  fi
  git -C "$dir" -c user.name="github-actions[bot]" \
    -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
    -c commit.gpgsign=false \
    commit -q -m "Update $name from run ${GITHUB_RUN_ID:-local}"
  if git -C "$dir" push -q "$remote" badges; then
    exit 0
  fi
  echo "push attempt $attempt failed" >&2
done
exit 1
