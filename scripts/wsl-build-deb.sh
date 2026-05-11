#!/usr/bin/env bash
set -eo pipefail
exec </dev/null

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WIN_SRC="${WIN_SRC:-$(dirname "$SCRIPT_DIR")}"
WSL_SRC="${WSL_SRC:-$HOME/src/WTranscriber}"

for tool in bun cargo git; do
  command -v "$tool" >/dev/null || { echo "[wsl] missing $tool in WSL PATH" >&2; exit 127; }
done

export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0='*'

mkdir -p "$(dirname "$WSL_SRC")"

if [ -d "$WSL_SRC/.git" ]; then
  echo "[wsl] updating $WSL_SRC from $WIN_SRC"
  git -C "$WSL_SRC" fetch --quiet "$WIN_SRC" HEAD
  git -C "$WSL_SRC" reset --hard --quiet FETCH_HEAD
else
  if [ -e "$WSL_SRC" ]; then
    echo "[wsl] removing stale $WSL_SRC (no .git)"
    rm -rf "$WSL_SRC"
  fi
  echo "[wsl] cloning $WIN_SRC -> $WSL_SRC"
  git clone --quiet "$WIN_SRC" "$WSL_SRC"
fi

cd "$WSL_SRC"
export CARGO_INCREMENTAL=1
unset SHERPA_ONNX_LIB_DIR SHERPA_ONNX_LIB SHERPA_ONNX_INCLUDE_DIR

# Redirect the build's stdio to a log file so wsl.exe's pipes are only held
# by our short-lived `tail` process. Without this, cargo/bun descendants keep
# the wsl.exe pipes open even after the build completes, causing wsl.exe to
# hang indefinitely.
LOG=$(mktemp -t wt-wsl-build.XXXXXX.log)
trap 'rm -f "$LOG"' EXIT

{
  bun install --no-progress 2>&1 | tail -5
  bun run tauri build --bundles deb -- --no-default-features --features sherpa-static
  echo "WT-RC=$?"
} >"$LOG" 2>&1 &
BUILD=$!

tail -F -n +1 "$LOG" 2>/dev/null &
TAIL=$!

wait "$BUILD"
RC=$?

sleep 1
kill "$TAIL" 2>/dev/null || true
wait "$TAIL" 2>/dev/null || true

MARKER_RC=$(sed -n 's/^WT-RC=//p' "$LOG" | tail -1)
exit "${MARKER_RC:-$RC}"
