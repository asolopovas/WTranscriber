set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]
set dotenv-load := false

_sep := if os() == 'windows' { ';' } else { ':' }
_home := if os() == 'windows' { env_var('USERPROFILE') } else { env_var('HOME') }
_android_sdk_default := if os() == 'windows' {
    _home / 'AppData' / 'Local' / 'Android' / 'Sdk'
} else {
    _home / 'Android' / 'Sdk'
}

export PATH := _home / '.cargo' / 'bin' + _sep + env_var('PATH')

_android_sdk := env_var_or_default('ANDROID_HOME', _android_sdk_default)
_android_ndk := env_var_or_default('NDK_HOME', _android_sdk / 'ndk' / '27.2.12479018')
export ANDROID_HOME := _android_sdk
export NDK_HOME := _android_ndk
export ANDROID_NDK := _android_ndk
export ANDROID_NDK_ROOT := _android_ndk
export ANDROID_NDK_HOME := _android_ndk
_libclang_default := if os() == 'windows' { 'C:\Program Files\LLVM\bin' } else { '/usr/lib/x86_64-linux-gnu' }
_libclang_env := env_var_or_default('LIBCLANG_PATH', '')
export LIBCLANG_PATH := if _libclang_env == '' { _libclang_default } else { _libclang_env }
export CMAKE_GENERATOR := env_var_or_default('CMAKE_GENERATOR', 'Ninja')

_run := "bun scripts/run.ts"
_par := "bun scripts/parallel.ts"

default:
    @just --list --unsorted

# ─── setup ────────────────────────────────────────────────────────────────────

[group('setup')]
setup:
    {{_run}} --tag setup --idle 60 --max 300 -- bun install
    @just install-hooks

[group('setup')]
install-hooks:
    git config core.hooksPath .githooks
    @echo "git hooks path → .githooks"

[windows, group('setup')]
bootstrap:
    {{_run}} --tag bootstrap --idle 120 --max 1800 -- powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/bootstrap-windows.ps1
    @bun -e "import {mkdirSync,writeFileSync} from 'node:fs'; mkdirSync('tmp',{recursive:true}); writeFileSync('tmp/.bootstrap.stamp', new Date().toISOString())"

# Run full bootstrap only if scripts/bootstrap-windows.ps1 is newer than tmp/.bootstrap.stamp.
[windows, private]
bootstrap-if-stale:
    @bun -e "import {statSync,existsSync} from 'node:fs'; const s='tmp/.bootstrap.stamp'; const src='scripts/bootstrap-windows.ps1'; const stale=!existsSync(s)||statSync(src).mtimeMs>statSync(s).mtimeMs; process.exit(stale?1:0)"; if ($LASTEXITCODE -ne 0) { just bootstrap }

# ─── develop ──────────────────────────────────────────────────────────────────

# Desktop HMR (Windows). Vite + tauri dev.
[windows, group('develop')]
dev:
    $env:RUST_BACKTRACE='1'; {{_run}} --tag dev --idle 0 --max 0 -- bun run tauri dev

# Desktop HMR (Linux/macOS). Vite + tauri dev.
[unix, group('develop')]
dev:
    RUST_BACKTRACE=1 {{_run}} --tag dev --idle 0 --max 0 -- bun run tauri dev

# Alias: Android USB HMR session (same as `just android`).
[group('develop')]
dev-android device="":
    {{_run}} --tag dev-android --idle 120 --max 2100 -- cargo xtask android bootstrap usb {{device}}

# Desktop HMR with sherpa-static (CPU-only, no CUDA runtime).
[windows, group('develop')]
dev-cpu:
    $env:RUST_BACKTRACE='1'; {{_run}} --tag dev-cpu --idle 0 --max 0 -- bun run tauri dev -- --no-default-features --features sherpa-static

# Headless rebuild loop on Rust source changes.
[group('develop')]
watch:
    {{_run}} --tag watch --idle 0 --max 0 -- cargo watch -w src-tauri/src --manifest-path src-tauri/Cargo.toml -x "build --release"

# Headless wt CLI (single shot).
[group('develop')]
cli *args:
    {{_run}} --tag cli --idle 60 --max 600 -- cargo run --manifest-path src-tauri/Cargo.toml --quiet --bin wt -- {{args}}

[group('develop')]
preview:
    {{_run}} --tag preview --idle 60 --max 60 -- bun run preview

[group('develop')]
typecheck:
    {{_run}} --tag typecheck --idle 60 --max 180 -- bun run typecheck

# ─── build ────────────────────────────────────────────────────────────────────

# Wipe per-tag build logs (logs/). Runs before every build recipe.
[private]
clean-logs:
    @bun -e "import {rmSync,mkdirSync} from 'node:fs'; rmSync('logs',{recursive:true,force:true}); mkdirSync('logs',{recursive:true})"

# Full release matrix: Windows host (GUI + CLI) + Linux .deb (Docker) + Android APK (Docker).
[windows, group('build')]
build: bootstrap-if-stale clean-logs
    {{_run}} --tag build --idle 600 --max 3600 -- cargo xtask release --dev

# Windows host only: NSIS .exe + wt CLI binary, built in parallel via xtask.
[windows, group('build')]
build-host: bootstrap-if-stale clean-logs
    {{_run}} --tag build-host --idle 600 --max 3600 -- cargo xtask release --dev --no-android --no-deb --no-windows-vm

# Linux .deb built inside the unified debian:12 builder container (glibc 2.36 floor).
[group('build')]
build-deb-docker: clean-logs
    {{_run}} --tag build-deb-docker --idle 180 --max 3600 -- cargo xtask release --dev --no-host --no-android --no-windows-vm

# ─── android: dev session ─────────────────────────────────────────────────────

# Bootstrap Android USB/emu HMR session (idempotent; no-ops if healthy).
[group('android')]
android device="":
    {{_run}} --tag android --idle 120 --max 2100 -- cargo xtask android bootstrap usb {{device}}

[group('android')]
android-host device="":
    {{_run}} --tag android-host --idle 120 --max 2100 -- cargo xtask android bootstrap host {{device}}

[group('android')]
android-stop device="":
    {{_run}} --tag android-stop --idle 30 --max 60 -- cargo xtask android stop {{device}}

[group('android')]
android-status device="":
    {{_run}} --tag android-status --idle 30 --max 30 -- cargo xtask android status {{device}}

[group('android')]
android-status-json device="":
    {{_run}} --tag android-status-json --idle 30 --max 30 -- cargo xtask android status --json {{device}}

[group('android')]
android-smoke device="":
    {{_run}} --tag android-smoke --idle 30 --max 60 -- cargo xtask android smoke {{device}}

[group('android')]
android-debug-attach device="":
    {{_run}} --tag android-attach --idle 15 --max 30 -- cargo xtask android attach {{device}}

[group('android')]
android-debug-eval expr:
    {{_run}} --tag android-eval --idle 15 --max 30 -- bun scripts/cdp.ts {{quote(expr)}}

# Headless x86_64 emulator (cross-platform; bounded boot wait, ≤180s).
[group('android')]
android-emu name="wt":
    {{_run}} --tag emu-start --idle 30 --max 240 -- bun scripts/android-emu.ts start --name {{name}}

[group('android')]
android-emu-stop:
    {{_run}} --tag emu-stop --idle 10 --max 30 -- bun scripts/android-emu.ts stop

# ─── android: build / install / cli ───────────────────────────────────────────

[group('android')]
android-targets:
    {{_run}} --tag a-targets --idle 60 --max 600 -- rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android

[group('android')]
android-prebuilts:
    {{_run}} --tag a-prebuilts --idle 60 --max 1800 -- cargo xtask android prebuilts

[group('android')]
android-init: android-targets android-prebuilts
    {{_run}} --tag a-init --idle 120 --max 1200 -- bun run tauri android init

[group('android')]
android-build target="aarch64":
    {{_run}} --tag a-build --idle 180 --max 1800 -- cargo xtask android build --target {{target}}

[group('android')]
android-install target="aarch64":
    {{_run}} --tag a-install --idle 180 --max 1800 -- cargo xtask android install --target {{target}}

[group('android')]
android-install-fresh target="aarch64":
    {{_run}} --tag a-install --idle 180 --max 1800 -- cargo xtask android install --target {{target}} --fresh

[group('android')]
android-doctor target="aarch64":
    {{_run}} --tag a-doctor --idle 30 --max 120 -- cargo xtask android doctor --target {{target}}

[group('android')]
android-cli target="aarch64":
    {{_run}} --tag a-cli --idle 180 --max 1800 -- cargo xtask android cli --target {{target}} --debug

[group('android')]
android-cli-push:
    {{_run}} --tag a-cli-push --idle 60 --max 300 -- cargo xtask android cli-push

[group('android')]
android-cli-run *args:
    {{_run}} --tag a-cli-run --idle 60 --max 300 -- cargo xtask android cli-run -- {{args}}

# ─── quality ──────────────────────────────────────────────────────────────────

[group('quality')]
fmt:
    {{_run}} --tag fmt --idle 30 --max 120 -- cargo fmt --manifest-path src-tauri/Cargo.toml --all
    {{_run}} --tag fmt-xtask --idle 30 --max 120 -- cargo fmt --manifest-path xtask/Cargo.toml --all
    {{_run}} --tag fmt-js --idle 30 --max 120 -- bun x prettier --write "src/**/*.{ts,vue}" "scripts/**/*.ts" "*.{json,html,md}" "docs/**/*.md"

[group('quality')]
fmt-check:
    {{_run}} --tag fmt-check --idle 30 --max 120 -- cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
    {{_run}} --tag fmt-check-xtask --idle 30 --max 120 -- cargo fmt --manifest-path xtask/Cargo.toml --all -- --check
    {{_run}} --tag fmt-check-js --idle 30 --max 120 -- bun x prettier --check "src/**/*.{ts,vue}" "scripts/**/*.ts" "*.{json,html,md}" "docs/**/*.md"

[group('quality')]
lint:
    {{_run}} --tag clippy --idle 120 --max 900 -- cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --offline -- -D warnings
    {{_run}} --tag clippy-xtask --idle 60 --max 300 -- cargo clippy --manifest-path xtask/Cargo.toml --all-targets --offline -- -D warnings
    {{_run}} --tag typecheck --idle 60 --max 180 -- bun run typecheck
    {{_run}} --tag vue-lint --idle 30 --max 120 -- bun run scripts/lint-vue.ts
    {{_run}} --tag knip --idle 30 --max 120 -- bun x knip

[group('quality')]
test:
    {{_run}} --tag rust-test --idle 90 --max 600 -- cargo test --manifest-path src-tauri/Cargo.toml --offline
    {{_run}} --tag xtask-test --idle 60 --max 300 -- cargo test --manifest-path xtask/Cargo.toml --offline
    {{_run}} --tag js-test --idle 60 --max 300 -- bun run test

# Playwright UI tests against the Vite dev server with mocked Tauri IPC.
[group('quality')]
e2e:
    {{_run}} --tag e2e --idle 60 --max 600 -- bun run test:ui

[group('quality')]
dep-check: _ensure-machete
    {{_run}} --tag machete --idle 30 --max 120 -- cargo machete src-tauri
    {{_run}} --tag machete-xtask --idle 30 --max 120 -- cargo machete xtask

[group('quality')]
audit: _ensure-audit
    {{_run}} --tag cargo-audit --idle 60 --max 300 -- cargo audit --file src-tauri/Cargo.lock
    {{_run}} --tag bun-audit --idle 30 --max 120 -- bun audit

# Pre-release gate: 11 jobs in parallel (fmt-check, clippy, clippy-xtask, typecheck, vue-lint, knip, rust-test, xtask-test, js-test, machete, audit).
[group('quality')]
check: _ensure-machete _ensure-audit
    {{_par}} --idle 600 --max 1800 \
        --job 'fmt-check=cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check && cargo fmt --manifest-path xtask/Cargo.toml --all -- --check && bun x prettier --check "src/**/*.{ts,vue}" "scripts/**/*.ts" "*.{json,html,md}" "docs/**/*.md"' \
        --job 'clippy=cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --offline -- -D warnings' \
        --job 'clippy-xtask=cargo clippy --manifest-path xtask/Cargo.toml --all-targets --offline -- -D warnings' \
        --job 'typecheck=bun run typecheck' \
        --job 'vue-lint=bun run scripts/lint-vue.ts' \
        --job 'knip=bun x knip' \
        --job 'rust-test=cargo test --manifest-path src-tauri/Cargo.toml --offline' \
        --job 'xtask-test=cargo test --manifest-path xtask/Cargo.toml --offline' \
        --job 'js-test=bun run test' \
        --job 'machete=cargo machete src-tauri && cargo machete xtask' \
        --job 'audit=cargo audit --file src-tauri/Cargo.lock && bun audit'
    @echo "✓ check passed"

# Desktop dev prerequisites check (rust ≥1.88, bun, just, hooks, audit config).
[group('quality')]
doctor:
    {{_run}} --tag doctor --idle 30 --max 60 -- bun scripts/doctor.ts

_ensure-machete:
    @bun scripts/ensure-cargo-tool.ts cargo-machete

_ensure-audit:
    @bun scripts/ensure-cargo-tool.ts cargo-audit

# ─── clean ────────────────────────────────────────────────────────────────────

[group('clean')]
clean:
    {{_run}} --tag clean-temp --idle 30 --max 120 -- bun scripts/clean-temp.ts --force
    {{_run}} --tag clean-rust --idle 60 --max 300 -- cargo clean --manifest-path src-tauri/Cargo.toml
    {{_run}} --tag clean-xtask --idle 30 --max 120 -- cargo clean --manifest-path xtask/Cargo.toml
    {{_run}} --tag clean-node --idle 30 --max 120 -- node -e "const{rmSync}=require('fs');for(const p of ['dist','node_modules']){try{rmSync(p,{recursive:true,force:true,maxRetries:3,retryDelay:100});console.log('removed '+p)}catch(e){console.error(p+': '+e.message);process.exit(1)}}"

# ─── icons ────────────────────────────────────────────────────────────────────

[group('build')]
icons source="src-tauri/icons/icon.png":
    {{_run}} --tag icons --idle 60 --max 300 -- bun run tauri icon {{source}}

# ─── runtime deps (Windows) ───────────────────────────────────────────────────

[windows, group('runtime-deps')]
cudnn version="9.21.1.3":
    {{_run}} --tag cudnn --idle 120 --max 1800 -- pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-cudnn.ps1 -Version {{version}}

[windows, group('runtime-deps')]
sherpa-cuda version="v1.13.0":
    {{_run}} --tag sherpa-cuda --idle 120 --max 1800 -- pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-sherpa-cuda.ps1 -Version {{version}}

[windows, group('runtime-deps')]
nemo-deps:
    {{_run}} --tag nemo-deps --idle 120 --max 1800 -- pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-nemo-deps.ps1

# ─── release ──────────────────────────────────────────────────────────────────

[group('release')]
release:
    {{_run}} --tag publish-dev --idle 180 --max 1800 -- cargo xtask publish dev

[group('release')]
release-stable level="patch":
    @just check
    {{_run}} --tag bump --idle 30 --max 120 -- cargo xtask bump {{level}}
    {{_run}} --tag release --idle 180 --max 3600 -- cargo xtask release
    {{_run}} --tag publish --idle 180 --max 1800 -- cargo xtask publish stable

[group('release')]
release-bump level="patch":
    {{_run}} --tag bump --idle 30 --max 120 -- cargo xtask bump {{level}}

[group('release')]
release-build *args:
    {{_run}} --tag release-build --idle 180 --max 3600 -- cargo xtask release {{args}}

[group('release')]
release-publish channel:
    {{_run}} --tag publish --idle 180 --max 1800 -- cargo xtask publish {{channel}}
