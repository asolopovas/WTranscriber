set windows-shell := ["pwsh.exe", "-NoLogoProfile", "-NoProfile", "-Command"]
set dotenv-load := false

export PATH := env_var('USERPROFILE') / '.cargo' / 'bin' + if os() == 'windows' { ';' } else { ':' } + env_var('PATH')

default:
    @just --list

setup:
    bun install

dev:
    bun run tauri dev

build:
    bun run tauri build

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

bump version:
    bun pm version {{version}} --no-git-tag-version
