#!/usr/bin/env bash
# Publish artifacts produced by scripts/release-build.mjs to a GitHub release.
# Usage: bash scripts/release-publish.sh <dev|stable>
#
# Stable: creates immutable tag vX.Y.Z (must already exist locally), pushes,
# creates GH release with --generate-notes, uploads artifacts.
# Dev:    force-updates rolling 'dev' tag + prerelease, replaces artifacts.
#
# msys2 bash on Windows strips Windows env (APPDATA/USERPROFILE), which breaks
# gh.exe auth lookup. Reconstruct GH_CONFIG_DIR like wt's release-publish.sh.
set -euo pipefail

if [ -z "${GH_CONFIG_DIR:-}" ] && command -v cygpath >/dev/null 2>&1; then
  cfg="$(cygpath -H)/$(whoami)/AppData/Roaming/GitHub CLI"
  [ -f "$cfg/hosts.yml" ] && export GH_CONFIG_DIR="$cfg"
fi

mode="${1:?usage: release-publish.sh <dev|stable>}"
case "$mode" in dev|stable) ;; *) echo "mode must be dev or stable" >&2; exit 1;; esac

list_file="releases/.release-${mode}-artifacts"
[ -f "$list_file" ] || { echo "ERROR: $list_file not found — run release-build first" >&2; exit 1; }

mapfile -t artifacts < "$list_file"
[ "${#artifacts[@]}" -gt 0 ] || { echo "ERROR: no artifacts in $list_file" >&2; exit 1; }
echo "artifacts: ${artifacts[*]}"

command -v gh >/dev/null 2>&1 || { echo "ERROR: gh CLI not found" >&2; exit 1; }

upload_with_retry() {
  local tag="$1"; shift
  local attempt=0
  until gh release upload "$tag" "$@" --clobber; do
    attempt=$((attempt + 1))
    if [ "$attempt" -ge 3 ]; then
      echo "ERROR: gh release upload failed after $attempt attempts" >&2
      exit 1
    fi
    echo "upload attempt $attempt failed; retrying in 5s..." >&2
    sleep 5
  done
}

if [ "$mode" = "stable" ]; then
  ver="$(node -p "require('./package.json').version")"
  tag="v$ver"

  if [ -n "$(git status --porcelain)" ]; then
    echo "ERROR: working tree dirty — refusing to publish stable" >&2; exit 1
  fi
  git rev-parse "$tag" >/dev/null 2>&1 || { echo "ERROR: tag $tag does not exist locally — run 'just release-bump' first" >&2; exit 1; }

  echo "--- pushing HEAD + tag $tag ---"
  git push origin HEAD
  git push origin "$tag"

  if gh release view "$tag" >/dev/null 2>&1; then
    echo "--- release $tag already exists; uploading additional artifacts ---"
  else
    echo "--- creating release $tag ---"
    gh release create "$tag" --title "$tag" --generate-notes
  fi

  upload_with_retry "$tag" "${artifacts[@]}"
  echo "✓ stable: https://github.com/asolopovas/WTranscriber/releases/tag/$tag"
else
  sha="$(git rev-parse --short HEAD)"
  branch="$(git rev-parse --abbrev-ref HEAD)"
  [ "$branch" = "HEAD" ] && branch="main"
  tag="dev"

  echo "--- updating $tag tag to $sha ---"
  git push origin HEAD
  git tag -f "$tag" HEAD
  git push origin "$tag" --force

  if gh release view "$tag" >/dev/null 2>&1; then
    echo "--- deleting existing $tag release ---"
    gh release delete "$tag" --yes --cleanup-tag=false
  fi

  echo "--- creating $tag prerelease ---"
  gh release create "$tag" \
    --title "Dev ($branch @ $sha)" \
    --prerelease \
    --notes "Rolling dev build of \`$branch\` at commit \`$sha\`. Artifacts replaced on every \`just release\`. Not a stable release; APK may be unsigned. SHA256SUMS attached."

  upload_with_retry "$tag" "${artifacts[@]}"
  echo "✓ dev: https://github.com/asolopovas/WTranscriber/releases/tag/$tag"
fi
