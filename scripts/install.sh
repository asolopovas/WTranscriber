#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
    '') ;;
    --help|-h)
        printf '%s\n' 'just install: bootstrap, build and install the current checkout on Windows or Linux.' \
            'Linux installs to ~/.local (override with WT_INSTALL_PREFIX); system dependencies may require sudo.' \
            'CUDA is selected automatically when supported; set WT_CUDA=0 for CPU or WT_CUDA=1 to require CUDA.' \
            'Use --interactive on Windows to show the installer UI. No git pull or branch switch is performed.'
        exit 0
        ;;
    *) printf 'install: unknown option: %s (use just install --help)\n' "$1" >&2; exit 2 ;;
esac
if (( $# > 1 )); then
    printf '%s\n' 'install: expected at most one option' >&2
    exit 2
fi
if [[ "$(uname -s)" != Linux ]]; then
    printf '%s\n' 'install: WTranscriber currently supports Windows and Linux desktop hosts.' >&2
    exit 1
fi
case "$(uname -m)" in
    x86_64|aarch64) ;;
    *) printf '%s\n' 'install: Linux requires x86_64 or aarch64.' >&2; exit 1 ;;
esac

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
if [[ -f tmp/_pids.json ]]; then
    printf '%s\n' 'install: stop the live dev session with just stop before building.' >&2
    exit 1
fi
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:${BUN_INSTALL:-$HOME/.bun}/bin:$PATH"

as_root() {
    if (( EUID == 0 )); then
        "$@"
    elif command -v sudo >/dev/null 2>&1; then
        sudo -- "$@"
    else
        printf '%s\n' 'install: system dependencies are missing; install sudo or ask your administrator to install them.' >&2
        exit 1
    fi
}

native_ready() {
    local cmd
    for cmd in cc c++ cmake ninja pkg-config curl unzip git; do
        command -v "$cmd" >/dev/null 2>&1 || return 1
    done
    pkg-config --exists gtk+-3.0 webkit2gtk-4.1 openssl || return 1
    ldconfig -p 2>/dev/null | grep 'libclang[.-]' >/dev/null || return 1
}

if ! native_ready; then
    printf '%s\n' 'install: installing native build dependencies'
    if command -v apt-get >/dev/null 2>&1; then
        as_root apt-get update
        as_root apt-get install -y build-essential curl ca-certificates unzip git file \
            cmake ninja-build pkg-config libclang-dev libssl-dev libwebkit2gtk-4.1-dev \
            libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev
    elif command -v dnf >/dev/null 2>&1; then
        as_root dnf install -y gcc gcc-c++ make curl ca-certificates unzip git file \
            cmake ninja-build pkgconf-pkg-config clang-devel openssl-devel webkit2gtk4.1-devel \
            gtk3-devel libappindicator-gtk3-devel librsvg2-devel libxdo-devel
    elif command -v pacman >/dev/null 2>&1; then
        as_root pacman -S --needed --noconfirm base-devel curl ca-certificates unzip git file \
            cmake ninja pkgconf clang openssl webkit2gtk-4.1 gtk3 libappindicator-gtk3 librsvg xdotool
    elif command -v zypper >/dev/null 2>&1; then
        as_root zypper --non-interactive install gcc gcc-c++ make curl ca-certificates unzip git file \
            cmake ninja pkg-config clang-devel libopenssl-devel webkit2gtk3-devel \
            gtk3-devel libappindicator3-devel librsvg-devel libXdo-devel
    else
        printf '%s\n' 'install: install the Tauri Linux prerequisites, CMake, Ninja, libclang, Git, curl and unzip for your distribution, then retry.' \
            'See https://v2.tauri.app/start/prerequisites/#linux' >&2
        exit 1
    fi
fi

install_tmp=$(mktemp -d)
trap 'rm -rf -- "$install_tmp"' EXIT
if ! command -v bun >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -fsSL https://bun.sh/install -o "$install_tmp/bun.sh"
    bash "$install_tmp/bun.sh"
fi
if ! command -v rustup >/dev/null 2>&1; then
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$install_tmp/rustup.sh"
    sh "$install_tmp/rustup.sh" -y --profile minimal --default-toolchain none --no-modify-path
fi

if [[ -n "${LIBCLANG_PATH:-}" && ! -d "$LIBCLANG_PATH" ]]; then
    unset LIBCLANG_PATH
fi
export CMAKE_GENERATOR="${CMAKE_GENERATOR:-Ninja}"
export GGML_NATIVE="${GGML_NATIVE:-OFF}"
export CARGO_TARGET_DIR="$PWD/src-tauri/target"
unset CARGO_BUILD_TARGET
bun install
rustup show active-toolchain
bun scripts/run.ts --tag install-build --idle 1800 --max 7200 -- \
    bun scripts/desktop.ts build
bun scripts/install-linux.ts
