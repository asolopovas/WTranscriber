# AGENTS.md

## Stack

Tauri 2 (Rust edition 2024, MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just`
(thin wrappers over `cargo xtask`).

## Layout

```
src/                    Vue 3 frontend
  api.ts                Tauri command wrappers
  types.ts              TS mirrors of Rust structs
src-tauri/src/
  main.rs               desktop binary
  bin/wt.rs             headless CLI
  lib.rs                tauri::Builder, plugins, invoke_handler
  commands.rs           #[tauri::command] handlers (thin)
  config.rs models.rs paths.rs error.rs
  transcriber/          transcription pipeline
xtask/src/              build / release / android orchestration
scripts/
  cdp.mjs               CDP eval against running WebView
  diarize.py            diarization sidecar (resource)
  install-*.ps1         Windows runtime deps (CUDA / cuDNN / NeMo)
docs/
  android.md            Android build + live UI dev (HMR)
  tauri-debug.md        WebView DevTools, CDP, logcat
  release.md            release process
  rust-build-speed.md   compile-time tuning
```

## Rules

- **No comments in code.** Names carry intent.
- **No `sleep` in scripts.** Wait on a real signal (process exit, file,
  log line, polled condition with timeout).
- **Edition 2024** (`LazyLock`, `let-else`, …).
- **Errors crossing Rust → JS** go through `error::Error` (`Serialize`).
- `src/types.ts` mirrors the Rust structs.
- Lints: `cargo clippy -- -D warnings` (pedantic + nursery on).
- `just check` must pass before commit; pre-commit hook enforces it.

## Daily recipes

```
just dev                desktop (HMR)
just android-dev        Android over USB (HMR via adb reverse)
just android-dev-host   Android over LAN (--host)
just check              fmt + lint + typecheck + test (offline)
just release-stable     check + bump + tag + build + publish
```

`just --list` for everything else. Release / build-speed / Android
specifics live in `docs/`.

## Adding a Tauri command

1. Handler in `src-tauri/src/commands.rs`.
2. Register in `lib.rs` `invoke_handler![…]`.
3. Typed wrapper in `src/api.ts`; domain types in `src/types.ts`.

## Android quick refs

- HMR design loop → `docs/android.md` § "Live UI dev".
- `just android-debug-attach` → forwards `tcp:9222` to the WebView; open
  `chrome://inspect`.
- `node scripts/cdp.mjs "<expr>"` → eval JS in the live WebView.
- Logcat: `chromium` / `Console` (JS), `RustStdoutStderr` (Rust).
- Screenshots under `tmp/` (root `*.png` is gitignored).
