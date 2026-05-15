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
export GGML_NATIVE := env_var_or_default('GGML_NATIVE', 'OFF')
export CMAKE_CUDA_ARCHITECTURES := env_var_or_default('CMAKE_CUDA_ARCHITECTURES', '75;80;86;89')
# MSVC cl.exe + parallel Ninja builds clash on shared .pdb files (C1041).
# `CL` is read by cl.exe as prepended flags on every invocation, bypassing
# any CMAKE_C_FLAGS_RELEASE overrides set by whisper.cpp / ggml CMakeLists.
# Empty string on non-Windows is harmless (no cl.exe to read it).
_cl_default := if os() == 'windows' { '/FS' } else { '' }
export CL := env_var_or_default('CL', _cl_default)

_run := "bun scripts/run.ts"
_xtask_dev_stop := "cargo run --quiet --manifest-path xtask/Cargo.toml --target-dir tmp/xtask-dev-stop-target -- dev stop"
_xtask_android_bootstrap := "cargo run --quiet --manifest-path xtask/Cargo.toml --target-dir tmp/xtask-android-bootstrap-target -- android bootstrap"

default:
    @just --list --unsorted

# ─── setup ────────────────────────────────────────────────────────────────────

setup:
    {{_run}} --tag setup --idle 60 --max 300 -- bun install
    git config core.hooksPath .githooks
    @echo "git hooks path → .githooks"

[windows]
bootstrap:
    {{_run}} --tag bootstrap --idle 120 --max 1800 -- powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/bootstrap-windows.ps1
    # Pre-warm cargo deps so the first `just check` after bootstrap doesn't
    # pay a ~5min cold whisper.cpp + ggml + sherpa-onnx C++ build under
    # parallel cargo invocations. Subsequent checks finish in <10s.
    {{_run}} --tag bootstrap-deps --idle 600 --max 1800 -- cargo build --manifest-path src-tauri/Cargo.toml
    @bun -e "import {mkdirSync,writeFileSync} from 'node:fs'; mkdirSync('tmp',{recursive:true}); writeFileSync('tmp/.bootstrap.stamp', new Date().toISOString())"

[windows, private]
bootstrap-if-stale:
    @bun -e "import {statSync,existsSync} from 'node:fs'; const s='tmp/.bootstrap.stamp'; const src='scripts/bootstrap-windows.ps1'; const stale=!existsSync(s)||statSync(src).mtimeMs>statSync(s).mtimeMs; process.exit(stale?1:0)"; if ($LASTEXITCODE -ne 0) { just bootstrap }

# ─── develop ──────────────────────────────────────────────────────────────────

# Desktop HMR (Vite + tauri dev). `just dev stop` stops any running dev session.
[windows]
dev action="":
    if ("{{action}}" -eq "stop") { {{_run}} --tag dev-stop --idle 30 --max 60 -- {{_xtask_dev_stop}} } else { {{_run}} --tag dev-stop --idle 30 --max 60 -- {{_xtask_dev_stop}}; $env:RUST_BACKTRACE='1'; {{_run}} --tag dev --idle 0 --max 0 -- bun run tauri dev }

[unix]
dev action="":
    if [ "{{action}}" = "stop" ]; then {{_run}} --tag dev-stop --idle 30 --max 60 -- {{_xtask_dev_stop}}; else {{_run}} --tag dev-stop --idle 30 --max 60 -- {{_xtask_dev_stop}}; RUST_BACKTRACE=1 {{_run}} --tag dev --idle 0 --max 0 -- bun run tauri dev; fi

# Android HMR session. mode = usb (default) or host. Always force-restarts.
android mode="usb" device="":
    {{_run}} --tag android --idle 120 --max 2100 -- {{_xtask_android_bootstrap}} {{mode}} {{device}}

# ─── quality ──────────────────────────────────────────────────────────────────

# Pre-release gate: all jobs by default, or selected jobs by tag.
check *jobs:
    {{_run}} --tag check --idle 600 --max 1800 -- cargo xtask check {{jobs}}

# Changed-file gate for hooks and CI.
check-changed *args:
    {{_run}} --tag check-changed --idle 120 --max 900 -- bun scripts/check-changed.ts {{args}}

# ─── build / release ──────────────────────────────────────────────────────────

[private]
clean-logs:
    @bun -e "import {rmSync,mkdirSync} from 'node:fs'; rmSync('logs',{recursive:true,force:true}); mkdirSync('logs',{recursive:true})"

# Full release matrix: Windows host + Linux .deb + Android APK → releases/dev/.
[windows]
build: bootstrap-if-stale clean-logs
    {{_run}} --tag build --idle 600 --max 3600 -- cargo xtask release --dev

# Windows host only: NSIS installer + bundled CLI → releases/dev/.
[windows]
build-host: bootstrap-if-stale clean-logs
    {{_run}} --tag build-host --idle 1800 --max 3600 -- cargo xtask release --dev --no-android --no-deb --no-windows-vm

# Publish releases/dev/ to the rolling gh `dev` prerelease.
release:
    {{_run}} --tag publish-dev --idle 180 --max 1800 -- cargo xtask publish dev

# Stable release: check + bump + build + publish.
release-stable level="patch":
    {{_run}} --tag release-stable --idle 180 --max 7200 -- cargo xtask release-stable {{level}}
