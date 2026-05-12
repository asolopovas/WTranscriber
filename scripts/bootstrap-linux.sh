#!/usr/bin/env bash
set -euo pipefail

log() { printf '\033[1;36m[bootstrap]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[bootstrap]\033[0m %s\n' "$*" >&2; }
err() { printf '\033[1;31m[bootstrap]\033[0m %s\n' "$*" >&2; }

need_cmd() { command -v "$1" >/dev/null 2>&1; }

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ "$(uname -s)" != "Linux" ]]; then
    err "this bootstrap is Linux-only (uname=$(uname -s))"
    exit 1
fi

if ! need_cmd apt-get; then
    warn "apt-get not found — skipping system package install (only Debian/Ubuntu is wired up)"
    SKIP_APT=1
else
    SKIP_APT=0
fi

if [[ "${SKIP_APT}" -eq 0 ]]; then
    log "checking system packages (Tauri + build deps)"
    pkgs=(
        build-essential
        pkg-config
        curl
        file
        unzip
        libssl-dev
        libgtk-3-dev
        libwebkit2gtk-4.1-dev
        libjavascriptcoregtk-4.1-dev
        libsoup-3.0-dev
        libayatana-appindicator3-dev
        librsvg2-dev
        clang
        libclang-dev
    )
    missing=()
    for p in "${pkgs[@]}"; do
        if ! dpkg -s "$p" >/dev/null 2>&1; then
            missing+=("$p")
        fi
    done
    apt_install_ok=1
    if (( ${#missing[@]} )); then
        log "installing: ${missing[*]}"
        sudo_pfx=()
        if [[ $EUID -ne 0 ]]; then
            sudo_pfx=(sudo -n)
        fi
        if "${sudo_pfx[@]}" apt-get update >/dev/null 2>&1 && "${sudo_pfx[@]}" apt-get install -y "${missing[@]}" >/dev/null 2>&1; then
            log "apt install succeeded"
        else
            warn "apt install needs sudo or password; falling back to local-extract for libclang"
            apt_install_ok=0
        fi
    else
        log "system packages already present"
    fi

    if [[ $apt_install_ok -eq 0 ]] || ! ls /usr/lib/x86_64-linux-gnu/libclang*.so* >/dev/null 2>&1; then
        if ! ls /usr/lib/x86_64-linux-gnu/libclang*.so* >/dev/null 2>&1; then
            cache="$ROOT/.bootstrap-cache/libclang"
            if ! ls "$cache"/usr/lib/x86_64-linux-gnu/libclang*.so* >/dev/null 2>&1; then
                log "downloading libclang-dev .deb to $cache (no sudo)"
                mkdir -p "$cache"
                (cd "$cache" && apt-get download libclang-dev libclang1 libclang-cpp-dev 2>/dev/null || true)
                shopt -s nullglob
                debs=("$cache"/*.deb)
                shopt -u nullglob
                if (( ${#debs[@]} == 0 )); then
                    err "apt-get download failed; cannot fetch libclang-dev without network or sources"
                    exit 1
                fi
                for d in "${debs[@]}"; do
                    dpkg-deb -x "$d" "$cache" >/dev/null
                done
            fi
            export LIBCLANG_PATH="$cache/usr/lib/x86_64-linux-gnu"
            mkdir -p "$ROOT/.bootstrap-cache"
            printf 'LIBCLANG_PATH=%s\n' "$LIBCLANG_PATH" > "$ROOT/.bootstrap-cache/env"
            log "libclang ready at $LIBCLANG_PATH"
        fi
    fi
fi

if ! need_cmd rustup; then
    log "installing rustup (stable toolchain)"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env"
else
    rustup toolchain list | grep -q stable || rustup install stable
fi
rustup default stable >/dev/null

MSRV="1.85"
if ! rustup toolchain list | grep -q "$MSRV"; then
    log "installing rust $MSRV (MSRV)"
    rustup toolchain install "$MSRV" --profile minimal
fi

if [[ "${WT_SKIP_ANDROID:-0}" != "1" ]]; then
    log "installing android rustup targets"
    rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android >/dev/null
    if [[ "${WT_ANDROID_PREBUILTS:-0}" == "1" ]]; then
        log "downloading sherpa-onnx android prebuilts (heavy, opt-in via WT_ANDROID_PREBUILTS=1)"
        cargo xtask android prebuilts
    fi
fi

if ! need_cmd bun; then
    log "installing bun"
    curl -fsSL https://bun.sh/install | bash
    export PATH="$HOME/.bun/bin:$PATH"
fi

if ! need_cmd just; then
    log "installing just"
    cargo install --locked just >/dev/null
fi

log "installing JS deps (bun install)"
bun install --frozen-lockfile 2>/dev/null || bun install

log "configuring git hooks"
git config core.hooksPath .githooks

if [[ "${WT_SKIP_VERIFY:-0}" != "1" ]]; then
    log "cargo check (sherpa-static, host)"
    cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --features sherpa-static
    log "bun typecheck"
    bun run typecheck
fi

log "bootstrap complete"
log "next: 'just dev-cpu' (desktop, sherpa static) or 'just android-bootstrap usb' (Android)"
