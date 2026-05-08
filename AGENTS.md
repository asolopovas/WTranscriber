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

## Live-dev workflow with monitoring subagents

The goal: edit code, see it on the device instantly, and get told the moment something breaks — without polling logs by hand.

### Three concurrent processes

1. **HMR dev session** — user runs in their own terminal:

   ```
   just android-dev          # USB / emulator
   just android-dev-host     # Wi-Fi / LAN
   ```

   Frontend-only watcher (`--no-watch` is on). Vue/TS/CSS pushed live; Rust changes ignored.

2. **CDP attach** — once, after the app launches:

   ```
   just android-debug-attach    # adb forward tcp:9222 → webview_devtools_remote_<pid>
   ```

   Required for both interactive eval and the error monitor.

3. **Error monitor subagent** — the agent spawns this as a long-running async delegate:
   ```
   node scripts/error-monitor.mjs
   ```
   What it captures, deduped and noise-filtered:
   - **Logcat (`*:W`)** — all `E`/`F` levels, `RustStdoutStderr` ERROR/WARN/panic, native crashes (`AndroidRuntime`, `tombstoned`).
   - **CDP runtime** — every JS `console.error`/`console.warn`, uncaught `pageerror` (with stack), failed network requests (`requestfailed`).
   - Drops known noise: reqwest/hyper connection chatter, HwcComposer/SurfaceFlinger/SemGameManager, `setRequestedFrameRate`, BufferQueue, ViewRootImpl, chatty dedup.
   - Self-deduplicates burst spam (same key within 2s collapsed).
   - Writes to stdout AND appends to `tmp/error-monitor.log` (gitignored).

### Agent loop

1. Spawn the monitor as an async delegate (`subagent` tool, `async: true`, `control.enabled: false` so the inactivity timeout never kills it).
2. Edit code. HMR pushes the change to the device.
3. Verify with `node scripts/cdp.mjs "<expr>"` (preferred) or PNG screenshot only for visual judgment.
4. The monitor reports failures — treat any new line in `tmp/error-monitor.log` as a regression signal and inspect immediately.
5. Backend change? Run `just android-install` in a separate terminal. The HMR session and the monitor both keep running; the monitor reattaches to the new WebView instance automatically (CDP retries for ~2 min).

### Pattern: starting the monitor

```
subagent({
  agent: "delegate",
  task: "node scripts/error-monitor.mjs\n\nStream forever. Surface any error/warn line back as a concise message. Ignore inactivity warnings.",
  async: true,
  cwd: "C:/Users/asolo/src/WTranscriber",
  control: { enabled: false },
})
```

### Why this pattern

- The dev session must run in the user's terminal so its `beforeDevCommand` console (Vite) stays attached and visible — spawning it through a subagent on Windows pops an empty conhost (`CREATE_NEW_PROCESS_GROUP` quirk).
- The monitor has no UI requirements; perfect fit for an async subagent.
- CDP eval gives the agent zero-friction inspection without round-tripping PNGs.

## Project subagents

Project-scoped agents live in `.pi/agents/` and keep the main thread clean by absorbing verbose work.

| Agent          | Purpose                                                                                                                                                                                                                                                                                                                                                                                 |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `doctor`       | Handles commits/pushes (runs the pre-commit gate, writes conventional-commit messages, pushes) **and** diagnostic forensics (test failures, CDP/logcat/monitor log triage). Always returns a single-screen verdict: `VERDICT` / `EVIDENCE` / `FIX`. Never dumps raw logs at the orchestrator. Use for: `"commit and push"`, `"why is X failing"`, `"what broke Y"`, `"run just check"`. |
| `wt-installer` | End-user install verification across Windows (GUI + CLI), Android, WSL.                                                                                                                                                                                                                                                                                                                 |
| `wt-tester`    | Functional smoke test with a 30-second clip across all platforms.                                                                                                                                                                                                                                                                                                                       |

### Delegation rules

- **Don't grep logs in the main thread.** Hand it to `doctor` with a focused question (`"diagnose: filename appears as 'primary:Recordings/...' after picker on Android"`). Doctor reads `tmp/error-monitor.log`, runs CDP probes, returns a verdict.
- **Don't run `just check` in the main thread.** Send it to `doctor` (`"run just check and report failures"`). The agent absorbs the multi-minute output and surfaces only what you must decide on.
- **All commits go through `doctor`.** Pass it the change summary; it stages, runs the gate, writes the message, pushes, returns the hash.
- The orchestrator stays focused on design intent and code structure; verbose tooling output never reaches it.
