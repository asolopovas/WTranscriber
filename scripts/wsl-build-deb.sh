#!/usr/bin/env bash
set -eo pipefail

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
  echo "[wsl] cloning $WIN_SRC -> $WSL_SRC"
  git clone --quiet "$WIN_SRC" "$WSL_SRC"
fi

cd "$WSL_SRC"

export CARGO_INCREMENTAL=1
unset SHERPA_ONNX_LIB_DIR SHERPA_ONNX_LIB SHERPA_ONNX_INCLUDE_DIR

bun install --no-progress 2>&1 | tail -5
bun run tauri build --bundles deb -- --no-default-features --features sherpa-static
