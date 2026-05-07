#!/usr/bin/env bash
set -euo pipefail
export MSYS_NO_PATHCONV=1

cmd="${1:-help}"
shift || true

WT_HOME=/data/local/tmp/wt-home
LIBS=/data/local/tmp/wtlibs
WT=/data/local/tmp/wt

case "$cmd" in
    push)
        ROOT="$(cd "$(dirname "$0")/.." && pwd)"
        BIN="$ROOT/src-tauri/target/aarch64-linux-android/debug/wt"
        if [[ ! -f "$BIN" ]]; then
            echo "build first: just android-cli"; exit 1
        fi
        adb shell mkdir -p "$LIBS" "$WT_HOME"
        adb push "$BIN" "${WT}-tmp" >/dev/null
        adb shell mv "${WT}-tmp" "$WT"
        adb shell chmod 755 "$WT"
        adb push "$ROOT/.android-prebuilt/jniLibs/arm64-v8a/." "$LIBS/" >/dev/null
        NDK="${NDK_HOME:-$LOCALAPPDATA/Android/Sdk/ndk/27.2.12479018}"
        adb push "$NDK/toolchains/llvm/prebuilt/windows-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$LIBS/" >/dev/null
        echo "pushed: $WT (LIBS=$LIBS, HOME=$WT_HOME)"
        ;;
    run|*)
        if [[ "$cmd" != "run" ]]; then set -- "$cmd" "$@"; fi
        adb shell "env LD_LIBRARY_PATH=$LIBS HOME=$WT_HOME XDG_CONFIG_HOME=$WT_HOME/.config XDG_DATA_HOME=$WT_HOME/.local/share XDG_CACHE_HOME=$WT_HOME/.cache $WT $*"
        ;;
esac
