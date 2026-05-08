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
  error-monitor.mjs     unified logcat + CDP console error stream
  diarize.py            diarization sidecar (resource)
  install-*.ps1         Windows runtime deps (CUDA / cuDNN / NeMo)
docs/
  android.md            Android build + live UI dev (HMR)
  dev-loop.md           HMR + error monitor + subagent delegation
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
- TS/Vue imports use path aliases (`@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`). No relative imports (`./`, `../`).
- Two-tier quality gate. Bugs are caught here, not later.
  - **Pre-commit (development):** the `.githooks/pre-commit` hook runs only the checks relevant to staged files — `cargo fmt --check` + `clippy -D warnings` for Rust, `prettier --check` + `vue-tsc` for TS/Vue, `prettier --check` for docs. Fast, scoped, mandatory. Never bypass with `--no-verify`.
  - **Pre-release (final gate):** `just check` is the single command that runs the full cross-cutting suite — fmt, clippy (pedantic + nursery, `-D warnings`), vue-tsc, vue lint, tests, dead-deps (`cargo machete`), and security audit (`cargo audit` + `bun audit`). It must pass before any release. `just release-stable` chains it in automatically; do not skip it.

## Commands

```
just dev                desktop (HMR)
just android-dev        Android over USB (HMR via adb reverse)
just android-dev-host   Android over LAN (--host)
just check              full pre-release gate: fmt + lint + typecheck + vue-lint + test + dep-check + audit
just release-stable     check + bump + tag + build + publish
```

`just --list` for everything else. Details in `docs/`.

## Adding a Tauri command

1. Handler in `src-tauri/src/commands.rs`.
2. Register in `lib.rs` `invoke_handler![…]`.
3. Typed wrapper in `src/api.ts`; domain types in `src/types.ts`.

## Android quick refs

- HMR design loop: see `docs/android.md` section "Live UI dev".
- `just android-dev` auto-detects the connected device's ABI; no `--target` flag (upstream `tauri android dev` doesn't accept one).
- Frontend / backend rebuilds are decoupled:
  - `just android-dev` runs with `--no-watch`. Vue/TS/CSS edits hot-reload instantly; the dev session never restarts.
  - Rust edits do **not** trigger anything. Rebuild on demand: `just android-install` in a second terminal. The dev session keeps streaming HMR while the app relaunches with the new native code.
  - Opt into Tauri's auto-Rust-rebuild with `cargo xtask android dev --watch`.
- **Diagnostics: prefer CDP over screenshots.**
  - `just android-debug-attach` forwards `tcp:9222` to the WebView.
  - `node scripts/cdp.mjs "<expr>"` evaluates JS in the live WebView — `getBoundingClientRect`, `getComputedStyle`, `outerHTML`, `querySelectorAll`, anything.
  - Use `adb exec-out screencap -p > tmp/screen.png` only when a _visual_ judgment is needed (fonts, animations, overall composition). Layout/spacing/colors/classes → always CDP.
- `chrome://inspect` for full DevTools (DOM tree, network, console, breakpoints).
- Logcat: `chromium` / `Console` (JS), `RustStdoutStderr` (Rust).
- Screenshots under `tmp/` (root `*.png` is gitignored).

## Dev loop

Three concurrent processes: HMR (`just android-dev` in user terminal) + CDP attach (`just android-debug-attach`, then `node scripts/cdp.mjs`) + error monitor (`node scripts/error-monitor.mjs` as async subagent). Backend rebuild = `just android-install` in second terminal; HMR + monitor keep running. Details in `docs/dev-loop.md`.

## Subagents (`.pi/agents/`)

- `doctor` — commits/pushes + test/log/CDP forensics. Returns `VERDICT` / `EVIDENCE` / `FIX`. Use for any commit, `just check`, log triage, regression diagnosis.
- `wt-installer` — install verification across Windows (GUI + CLI), Android, WSL.
- `wt-tester` — 30-second-clip smoke test across platforms.

Main thread stays on design + code; verbose tooling output goes through `doctor`.
