set windows-shell := ["pwsh.exe", "-NoLogo", "-NoProfile", "-Command"]
set dotenv-load := false

_sep := if os() == 'windows' { ';' } else { ':' }
export PATH := env_var('USERPROFILE') / '.cargo' / 'bin' + _sep + env_var('PATH')

default:
    @just --list

setup:
    bun install

dev:
    bun run tauri dev

build:
    bun run tauri build

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

lint:
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings
    bun run typecheck

test:
    cargo test --manifest-path src-tauri/Cargo.toml --all-features

clean:
    cargo clean --manifest-path src-tauri/Cargo.toml
    rm -rf dist node_modules

icons source="src-tauri/icons/icon.png":
    bun run tauri icon {{source}}

cudnn version="9.21.1.3":
    pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/install-cudnn.ps1 -Version {{version}}

sherpa-cuda-build version="v1.13.0":
    pwsh.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File scripts/build-sherpa-cuda.ps1 -Version {{version}}

bump version:
    bun pm version {{version}} --no-git-tag-version
