# AGENTS.md

Notes for agents working on this repo.

## Stack

- Tauri 2 (Rust, edition 2024, MSRV 1.85)
- Vue 3 + TypeScript + Vite
- Bun (JS runtime + package manager)
- `just` (task runner) → delegates to `cargo xtask` for non-trivial work

## Layout

```
src/                    Vue 3 frontend
  api.ts                wrappers around Tauri commands
  types.ts              TS types matching Rust structs
src-tauri/
  src/
    main.rs             desktop binary
    bin/wt.rs           headless CLI (clap)
    lib.rs              tauri::Builder, plugin + command registration
    commands.rs         #[tauri::command] handlers (thin)
    config.rs           persisted user config
    models.rs           model registry / discovery
    paths.rs            config / data paths (LazyLock)
    error.rs            thiserror Error, Serialize for IPC
    transcriber/        transcription pipeline
  capabilities/         Tauri permissions
  tauri.conf.json
xtask/src/              build / release orchestration (cargo xtask)
  release.rs            parallel host + Android + WSL builds, manifest
  bump.rs               version sync across package.json / Cargo.toml / tauri.conf.json
  publish.rs            gh CLI wrapper (dev / stable releases)
  android.rs            build / dev / install / cli / prebuilts / sign-patch
scripts/
  cdp.mjs               Chrome DevTools Protocol eval (debug)
  diarize.py            speaker diarization sidecar (runtime resource)
  install-*.ps1         Windows-only runtime deps (CUDA / cuDNN / NeMo)
docs/
  android.md            Android build + live UI dev (HMR)
  tauri-debug.md        WebView DevTools, CDP, logcat
  release.md            release process + build-speed notes
  rust-build-speed.md   compile-time tuning
```

## Rules

- **No comments in code.** Names carry intent.
- **No `sleep` in scripts.** Wait on a real signal: process exit, file
  appears, log line, or poll with timeout. Applies to bash, `.mjs`,
  `.ps1`, `adb shell`.
- **Edition 2024** features (`LazyLock`, `let-else`, …).
- **Errors crossing Rust → JS** go through `error::Error` (impl `Serialize`).
- **Frontend types** in `src/types.ts` must match the Rust structs.
- **Lints**: `cargo clippy -- -D warnings`, pedantic + nursery on.
- **Formatters**: `cargo fmt`, `prettier` (TS/Vue/MD/JSON/HTML).

## Recipes (`just --list` for the full set)

```
# setup
just setup              bun install + git hooks
just android-init       rustup targets + sherpa prebuilts + tauri android init

# develop
just dev                desktop (Vue HMR + tauri dev)
just watch              cargo watch, rebuild on save
just android-dev        Android dev on USB device (HMR via adb reverse)
just android-dev-host   Android dev over LAN (--host, sets TAURI_DEV_HOST)

# build
just build-bin          raw cargo build       (~6 s warm)
just build-app          tauri exe, no bundle  (~9 s warm)
just build              NSIS installer        (~28 s warm)
just build-cuda         build with --features cuda
just build-cli          headless `wt` CLI
just android-build      APK (aarch64 default)

# quality
just check              fmt-check + lint + test            (pre-commit)
just check-all          + cargo-machete + cargo-audit + bun audit

# release (delegate to cargo xtask)
just release            dev build → rolling 'dev' prerelease
just release-stable     check + bump + tag + build + publish
just release-bump       bump + commit + tag only
just release-build      artifacts only (--dev for dev channel)
just release-publish    upload existing artifacts
                        cargo xtask {release|bump|publish|android} --help
                        full reference: docs/release.md
```

## Quality gates

`just check` (fast, offline, runs pre-commit):

1. `cargo fmt --check` + `prettier --check`
2. `cargo clippy --all-targets --offline -- -D warnings`
3. `bun run typecheck` (`vue-tsc`)
4. `cargo test --offline`

`just check-all` adds `cargo machete`, `cargo audit`, `bun audit`.
Missing tools auto-install on first run.

## Git hooks (`.githooks/`)

Only relevant work runs.

- **pre-commit** by staged paths:
  - Rust / `Cargo.{toml,lock}` → `cargo fmt --check` + clippy
  - TS / Vue → `prettier --check` + `vue-tsc`
  - Markdown / JSON / HTML → `prettier --check`
- **pre-push** → `cargo test --offline` once.

`just setup` (or `just install-hooks`) sets `core.hooksPath = .githooks`.
Bypass with `--no-verify` only in emergencies.

## Adding a Tauri command

1. Function in `src-tauri/src/commands.rs` (or a domain module that
   re-exports it).
2. Register in `lib.rs` `invoke_handler![…]`.
3. Typed wrapper in `src/api.ts`.
4. Domain return type → add to `src/types.ts`.

## Android

- Build / link / prebuilts: `docs/android.md`.
- Live UI dev with HMR (no rebuild/reinstall on UI edits): same doc,
  "Live UI dev" section.
- WebView debugging (chrome://inspect, CDP, logcat, screenshots):
  `docs/tauri-debug.md`.

Quick reference:

- `just android-debug-attach` → forwards `tcp:9222` to the WebView
  devtools socket; open `chrome://inspect`.
- `node scripts/cdp.mjs "<expr>"` → eval JS in the live WebView.
- Logcat tags: `chromium` / `Console` (JS), `RustStdoutStderr` (Rust).
- Screenshots: `MSYS_NO_PATHCONV=1 adb exec-out screencap -p > tmp/x.png`
  (`*.png` at repo root is gitignored — keep under `tmp/`).
