set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]
set dotenv-load := false

_sep := if os() == 'windows' { ';' } else { ':' }
export PATH := env_var('USERPROFILE') / '.cargo' / 'bin' + _sep + env_var('PATH')

_android_sdk := env_var_or_default('ANDROID_HOME', env_var('USERPROFILE') / 'AppData' / 'Local' / 'Android' / 'Sdk')
_android_ndk := env_var_or_default('NDK_HOME', _android_sdk / 'ndk' / '27.2.12479018')
export ANDROID_HOME := _android_sdk
export NDK_HOME := _android_ndk
export ANDROID_NDK := _android_ndk
export ANDROID_NDK_ROOT := _android_ndk
export ANDROID_NDK_HOME := _android_ndk
export LIBCLANG_PATH := env_var_or_default('LIBCLANG_PATH', if os() == 'windows' { 'C:\Program Files\LLVM\bin' } else { '' })
export CMAKE_GENERATOR := env_var_or_default('CMAKE_GENERATOR', 'Ninja')

default:
    @just --list

setup:
    bun install
    @just install-hooks

install-hooks:
    git config core.hooksPath .githooks
    @echo "git hooks path -> .githooks"

# develop
dev:
    bun run tauri dev

dev-cpu:
    bun run tauri dev -- --no-default-features --features sherpa-static

watch:
    cargo watch -w src-tauri/src --manifest-path src-tauri/Cargo.toml -x "build --release"

# build
build:
    bun run tauri build

build-bin:
    cargo build --manifest-path src-tauri/Cargo.toml --release --bin wtranscriber

build-app:
    bun run tauri build --no-bundle

build-all:
    bun run tauri build --bundles nsis --bundles msi

build-cpu:
    bun run tauri build -- --no-default-features --features sherpa-static

build-cli:
    cargo build --manifest-path src-tauri/Cargo.toml --release --bin wt

# android
android-targets:
    rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

android-prebuilts:
    cargo xtask android prebuilts

android-init: android-targets android-prebuilts
    bun run tauri android init

android-dev device="":
    cargo xtask android dev {{device}}
android-dev-host device="":
    cargo xtask android dev --host {{device}}

android-build target="aarch64":
    cargo xtask android build --target {{target}}

android-install target="aarch64":
    cargo xtask android install --target {{target}}

android-install-fresh target="aarch64":
    cargo xtask android install --target {{target}} --fresh

android-doctor target="aarch64":
    cargo xtask android doctor --target {{target}}
    @rustup target list --installed

android-cli target="aarch64":
    cargo xtask android cli --target {{target}} --debug

android-cli-push:
    cargo xtask android cli-push

android-cli-run *args:
    cargo xtask android cli-run -- {{args}}

android-debug-attach:
    @bash -c 'set -e; export MSYS_NO_PATHCONV=1; \
      pid=$(adb shell cat /proc/net/unix | grep -oE "webview_devtools_remote_[0-9]+" | head -1 | sed "s/.*_//"); \
      [ -z "$pid" ] && { echo "no WebView devtools socket; is the app running?"; exit 1; }; \
      adb forward --remove tcp:9222 2>/dev/null || true; \
      adb forward tcp:9222 localabstract:webview_devtools_remote_$pid >/dev/null; \
      echo "forwarded tcp:9222 -> webview_devtools_remote_$pid"; \
      curl -s http://localhost:9222/json/list | node -e "let d=\"\"; process.stdin.on(\"data\",x=>d+=x).on(\"end\",()=>JSON.parse(d).forEach(p=>console.log(p.title,\"->\",p.url)))"'

android-debug-eval expr:
    @node scripts/cdp.mjs {{quote(expr)}}

# headless cli
cli *args:
    cargo run --manifest-path src-tauri/Cargo.toml --quiet --bin wt -- {{args}}

# frontend
preview:
    bun run preview

typecheck:
    bun run typecheck

# quality
fmt:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all
    bun x prettier --write "src/**/*.{ts,vue}" "*.{json,html,md}"

fmt-check:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
    bun x prettier --check "src/**/*.{ts,vue}" "*.{json,html,md}"

lint:
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --offline -- -D warnings
    bun run typecheck
    bun run scripts/lint-vue.mjs

test:
    cargo test --manifest-path src-tauri/Cargo.toml --offline

[windows]
_ensure-machete:
    @pwsh -NoLogo -NoProfile -Command "if (-not (Get-Command cargo-machete -EA SilentlyContinue)) { Write-Host 'installing cargo-machete' -ForegroundColor Cyan; cargo install --locked cargo-machete }"

[unix]
_ensure-machete:
    @command -v cargo-machete >/dev/null 2>&1 || cargo install --locked cargo-machete

[windows]
_ensure-audit:
    @pwsh -NoLogo -NoProfile -Command "if (-not (Get-Command cargo-audit -EA SilentlyContinue)) { Write-Host 'installing cargo-audit' -ForegroundColor Cyan; cargo install --locked cargo-audit }"

[unix]
_ensure-audit:
    @command -v cargo-audit >/dev/null 2>&1 || cargo install --locked cargo-audit

dep-check: _ensure-machete
    cargo machete src-tauri

audit: _ensure-audit
    cargo audit --file src-tauri/Cargo.lock --ignore RUSTSEC-2024-0413 --ignore RUSTSEC-2024-0416 --ignore RUSTSEC-2024-0412 --ignore RUSTSEC-2024-0418 --ignore RUSTSEC-2024-0411 --ignore RUSTSEC-2024-0417 --ignore RUSTSEC-2024-0414 --ignore RUSTSEC-2024-0415 --ignore RUSTSEC-2024-0420 --ignore RUSTSEC-2024-0419 --ignore RUSTSEC-2024-0370 --ignore RUSTSEC-2025-0081 --ignore RUSTSEC-2025-0075 --ignore RUSTSEC-2025-0080 --ignore RUSTSEC-2025-0100 --ignore RUSTSEC-2025-0098 --ignore RUSTSEC-2024-0429
    bun audit

check: fmt-check lint test dep-check audit
    @echo "✓ check passed — fmt, lint, typecheck, vue-lint, test, dead-deps, audit"

check-all: check
    @echo "✓ check-all passed (alias for check)"

clean:
    cargo clean --manifest-path src-tauri/Cargo.toml
    cargo clean --manifest-path xtask/Cargo.toml
    rm -rf dist node_modules

icons source="src-tauri/icons/icon.png":
    bun run tauri icon {{source}}

# windows-only runtime deps (CUDA, cuDNN, NeMo)
cudnn version="9.21.1.3":
    pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-cudnn.ps1 -Version {{version}}

sherpa-cuda version="v1.13.0":
    pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-sherpa-cuda.ps1 -Version {{version}}

nemo-deps:
    pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-nemo-deps.ps1

# release
release:
    cargo xtask release --dev
    cargo xtask publish dev

release-stable level="patch":
    @just check
    cargo xtask bump {{level}}
    cargo xtask release
    cargo xtask publish stable

release-bump level="patch":
    cargo xtask bump {{level}}

release-build *args:
    cargo xtask release {{args}}

release-publish channel:
    cargo xtask publish {{channel}}
