set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]
set dotenv-load := false

_sep := if os() == 'windows' { ';' } else { ':' }
export PATH := env_var('USERPROFILE') / '.cargo' / 'bin' + _sep + env_var('PATH')

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

build:
    bun run tauri build

build-cuda:
    bun run tauri build -- --no-default-features --features cuda

build-cli:
    cargo build --manifest-path src-tauri/Cargo.toml --release --bin wt

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

bump version:
    bun pm version {{version}} --no-git-tag-version
