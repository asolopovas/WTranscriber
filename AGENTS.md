# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.85) · Vue 3 + TS + Vite · Bun · `just` (wraps `cargo xtask`).

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

- No comments in code. Names carry intent.
- No `sleep` in scripts. Wait on a real signal (process exit, file, log line, polled condition with timeout).
- Edition 2024. Use `LazyLock`, `let-else`, etc.
- Errors returned from Rust to JS must use `error::Error` (implements `Serialize`).
- `src/types.ts` mirrors Rust structs.
- Run `just check` before every commit and fix everything it reports. It runs fmt, clippy (pedantic + nursery, `-D warnings`), vue-tsc, vue lint, tests, dead-deps (`cargo machete`), and security audit (`cargo audit` + `bun audit`). The pre-commit hook enforces it. Bugs are caught here, not later.

## Commands

```
just dev                desktop (HMR)
just android-dev        Android over USB (HMR via adb reverse)
just android-dev-host   Android over LAN (--host)
just check              fmt + lint + typecheck + vue-lint + test + dep-check + audit
just release-stable     check + bump + tag + build + publish
```

`just --list` for everything else. Details in `docs/`.

## Adding a Tauri command

1. Handler in `src-tauri/src/commands.rs`.
2. Register in `lib.rs` `invoke_handler![…]`.
3. Typed wrapper in `src/api.ts`; domain types in `src/types.ts`.

## Android quick refs

- HMR design loop: see `docs/android.md` section "Live UI dev".
- `just android-debug-attach` forwards `tcp:9222` to the WebView; then open `chrome://inspect`.
- `node scripts/cdp.mjs "<expr>"` evaluates JS in the live WebView.
- Logcat: `chromium` / `Console` (JS), `RustStdoutStderr` (Rust).
- Screenshots under `tmp/` (root `*.png` is gitignored).
