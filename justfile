set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]
set dotenv-load := false

_sep := if os() == 'windows' { ';' } else { ':' }
export PATH := env_var('USERPROFILE') / '.cargo' / 'bin' + _sep + env_var('PATH')

_android_sdk := env_var_or_default('ANDROID_HOME', env_var('USERPROFILE') / 'AppData' / 'Local' / 'Android' / 'Sdk')
_android_ndk := env_var_or_default('NDK_HOME', _android_sdk / 'ndk' / '27.2.12479018')
_android_prebuilt := justfile_directory() / '.android-prebuilt'
export ANDROID_HOME := _android_sdk
export NDK_HOME := _android_ndk

default:
    @just --list

setup:
    bun install
    @just install-hooks

install-hooks:
    git config core.hooksPath .githooks
    @echo "git hooks path -> .githooks"

dev:
    bun run tauri dev

dev-cpu:
    bun run tauri dev -- --no-default-features --features sherpa-static

build:
    bun run tauri build

build-cpu:
    bun run tauri build -- --no-default-features --features sherpa-static

build-cli:
    cargo build --manifest-path src-tauri/Cargo.toml --release --bin wt

android-targets:
    rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

android-prebuilts:
    pwsh -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-android-prebuilts.ps1

android-init: android-targets android-prebuilts
    bun run tauri android init

android-dev target="aarch64":
    bun scripts/android.mjs dev --target={{target}}

android-build target="aarch64":
    bun scripts/android.mjs build --target={{target}} --release

android-build-debug target="aarch64":
    bun scripts/android.mjs build --target={{target}} --debug

android-doctor target="aarch64":
    bun scripts/android.mjs doctor --target={{target}}
    @rustup target list --installed

android-cli target="aarch64":
    bun scripts/android.mjs cli --target={{target}} --debug

android-cli-push: android-cli
    bash scripts/android-wt.sh push

android-cli-run *args:
    bash scripts/android-wt.sh run {{args}}

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

cli *args:
    cargo run --manifest-path src-tauri/Cargo.toml --quiet --bin wt -- {{args}}

preview:
    bun run preview

typecheck:
    bun run typecheck

fmt:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all
    bun x prettier --write "src/**/*.{ts,vue}" "*.{json,html,md}"

fmt-check:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
    bun x prettier --check "src/**/*.{ts,vue}" "*.{json,html,md}"

lint:
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --offline -- -D warnings
    bun run typecheck

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
    cargo audit --file src-tauri/Cargo.lock
    bun audit

check: fmt-check lint test
    @echo "✓ check passed (run 'just check-all' before release for audit + dep-check)"

check-all: check dep-check audit
    @echo "✓ check-all passed"

clean:
    cargo clean --manifest-path src-tauri/Cargo.toml
    rm -rf dist node_modules

icons source="src-tauri/icons/icon.png":
    bun run tauri icon {{source}}

cudnn version="9.21.1.3":
    pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-cudnn.ps1 -Version {{version}}

sherpa-cuda version="v1.13.0":
    pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-sherpa-cuda.ps1 -Version {{version}}

nemo-deps:
    pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-nemo-deps.ps1

bump version:
    bun pm version {{version}} --no-git-tag-version
