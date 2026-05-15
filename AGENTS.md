# AGENTS.md

Stack: Tauri 2 · Rust edition 2024 (MSRV 1.88, pinned via `rust-toolchain.toml`) · Vue 3 + TS + Vite · Bun · `just`.

## Layout

```
src/             Vue 3 frontend (api.ts, types.ts mirrors Rust)
src-tauri/src/   commands/ (per-domain), lib.rs (invoke_handler!), bin/wt.rs,
                 api.rs, config.rs, paths.rs, error.rs, constants.rs,
                 android.rs, browser.rs, essentials.rs, fs_utils.rs,
                 lang_id.rs, logfile.rs, process.rs, progress.rs,
                 runtime_install.rs,
                 models/, transcriber/, diarizer/, audio/, audio_toolkit/,
                 runtimes/, llm/, engine/, namer/
src-tauri/       tauri.conf.json, capabilities/default.json, gen/android/
xtask/src/       check / bump / publish / release / android orchestration
scripts/         run.ts, android-emu.ts, android-install.ts, cdp.ts,
                 clean-temp.ts, clear-dev-logs.ts, dev-vital.ts, doctor.ts,
                 check-changed.ts, lint-vue.ts, install-*.ps1,
                 bootstrap-windows.ps1, wt-windows-build.bat
docs/            android · dev-loop · release · rust-build-speed · tmp · asr-pipeline-v2
.agents/skills/  project-local pi skills (mirrored under .opencode/skills for opencode)
.vscode/         tasks.json (android build+install, dev, check, …)
```

## Task contract

Every `just` recipe runs through `scripts/run.ts` (Bun + TypeScript): line-prefixed output, heartbeat after 10 s of silence, kill on idle (default 90 s) or hard timeout (default 600 s), final `OK in X.Ys` / `FAIL exit=N in X.Ys`. The long-running `just dev` uses `--idle 0 --max 0`; `just android` is finite (it bootstraps a detached session and exits) but uses `--idle 120 --max 2100` to absorb cold aarch64-android cargo + first-run gradle (10–30 min). Anything quiet >30 s during steady state is a bug.

## Commands

```
just dev               desktop HMR (Linux/Windows); `just dev stop` to stop
just android           Android USB/host HMR session (clean restart)
just check             pre-release gate (11 jobs in parallel; accepts job tags)
just check-changed     changed-file gate for pre-commit/CI
just build             Windows-only dev release matrix → releases/dev/
just release           publish releases/dev/ to the rolling gh `dev` prerelease
just release-stable    check + bump + build + publish (stable)
just bootstrap         Windows host toolchain install + dep prewarm
just setup             bun install + git hooks
```

Android-only APK (no full release matrix): `bun scripts/android-install.ts` (build + adb install -r; add `--force` to handle keystore signature mismatch). Same via `.vscode/tasks.json` → "android: build + install APK". Headless emulator: `bun scripts/android-emu.ts`.

`just build` is Windows-only and runs `cargo xtask release --dev`: builds the Windows NSIS installer, the Android APK, and the Linux `.deb` (Docker) in parallel into `releases/dev/`. On Linux, run `cargo xtask release --dev` directly to use the `windowsVm` entry in `release.config.json`. Self-healing on transient Windows-VM failures uses the configured VM start/restart commands + 1 retry. `just release` is publish-only (`cargo xtask publish dev`); it never builds. See [`docs/release.md`](docs/release.md) for the failsafe + recovery flow.

`just check` runs `cargo xtask check`, which fans out **11 jobs** in parallel: `fmt-check`, `clippy`, `clippy-xtask`, `typecheck`, `vue-lint`, `knip`, `rust-test`, `xtask-test`, `js-test`, `machete`, `audit`. All jobs complete before the first failure is reported. Pass job tags for a focused run, e.g. `just check typecheck js-test`.

CI (`.github/workflows/check.yml`) runs `just check-changed --base …`: formatting/lint/typecheck/tests/audits are selected from the changed files, while full native Rust/Tauri gates stay local/release-only.

`just check` assumes the C++ deps (`whisper-rs-sys`, `sherpa-onnx-sys`) are already built — `just bootstrap` pre-warms them via `cargo build` after the system-deps script. Warm `just check` finishes in <10 s; a cold first run is ~5 min. If `target/` is wiped (`cargo clean`, fresh checkout, deleted `tmp/.bootstrap.stamp`), re-run `just bootstrap` rather than letting `just check` pay the cold rebuild under parallel cargo lock contention.

`just --list` for the rest.

## Conventions

- Rust edition 2024 (`LazyLock`, `let-else`); errors crossing JS use `error::Error` (`Serialize`).
- Tauri process split: Vue/WebView owns presentation; Rust owns filesystem, models, native, long work. Cross only via commands/events.
- `src/types.ts` mirrors Rust structs. Use aliases `@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`.
- New Tauri command = `commands/<domain>.rs` handler + `lib.rs` `invoke_handler![…]` (full path) + `api.ts` wrapper + `types.ts` mirror + `src-tauri/capabilities/default.json` permission if it touches a plugin/API.
- Large binary payloads cross IPC as raw bodies (`tauri::ipc::Request<'_>` / `Response`), not base64 strings — see `commands/audio_files.rs::save_recording`.
- Capability permissions: least-privilege. If IPC fails, inspect console + `RustStdoutStderr` before widening.
- No comments in code. No `sleep` in scripts; poll with timeout.
- Conventional commits, simple British English.

## Pre-commit hook

`.githooks/pre-commit` is mandatory; `--no-verify` is forbidden (the sole exception is the release bump commit, which runs after `just check`). Auto-formats touched files (Rust via `cargo fmt` for `src-tauri`/`xtask`, TS/Vue/scripts/docs via `prettier --write`), re-stages, then runs `bun scripts/check-changed.ts --staged`. The hook covers changed-file formatting, TS/Vue typecheck/tests/lint, xtask clippy/tests, and dependency audits when lockfiles change. Compile-heavy src-tauri Rust correctness stays in explicit `just check` and release gates.

## Tauri 2.11 patches

In-tree workarounds for upstream issues until Tauri 2.12 lands:

- `src-tauri/gen/android/app/src/main/java/com/asolopovas/wtranscriber/generated/WryActivity.kt` carries inline `@Suppress("DEPRECATION")` annotations on the `onDestroy`/`onRestart` overrides so they don't fail `-Werror` Kotlin builds.
- `xtask/src/android/patch.rs::patch_plugin_consumer_rules` touches an empty `consumer-rules.pro` inside each plugin's `android/` dir referenced by `gen/android/tauri.settings.gradle` (covers `tauri-plugin-dialog`, `tauri-plugin-fs`). Wired into `prepare()` before every Android build.
- `src-tauri/build.rs::stub_windows_bundle_resources` touches the Windows bundle placeholder needed by `tauri_build` resource validation during `just check` / dev builds on a fresh checkout. `install_cuda_dlls` then copies real CUDA DLLs from `%APPDATA%` during release builds. Pre-bundle: verify file sizes before shipping a release.
- `src-tauri/build.rs` warns when `CMAKE_GENERATOR` changes; `xtask/src/check.rs` owns the cache wipe for `target/{debug,release}/build/{whisper-rs-sys-*,sherpa-onnx-sys-*}` using the `target/.cmake-generator` sentinel.
- `xtask/src/release/builders.rs::ensure_dev_keystore_properties` regenerates `src-tauri/gen/android/keystore.properties` whenever the recorded `storeFile` is missing on the current host (so the same checkout signs APKs on Windows and Linux). Called from both `cargo xtask android build` and the release matrix.

Drop these once Tauri 2.12 publishes the fixed plugin gradle + activity migration.

## Windows host setup

`scripts/bootstrap-windows.ps1` (run by `just bootstrap`, which is a dependency of `just build`) installs/repairs: VS 2022 Build Tools, rustup (msvc), Bun, Node, NSIS, CMake, Ninja, LLVM/libclang, MSYS2, just, **CUDA Toolkit 12.x** (via `Nvidia.CUDA`), **cuDNN 9** (via `scripts/install-cudnn.ps1`), and **sherpa-onnx CUDA runtime** (via `scripts/install-sherpa-cuda.ps1`). Subsequent runs are idempotent. `bun scripts/doctor.ts` validates the same prerequisites are reachable from the current shell.

## Scratch artefacts

- `logs/<tag>.log` — per-tag build logs written by `scripts/run.ts`. **Wiped on every `just build`.**
- `tmp/` — dev-loop source of truth (PIDs, logcat, android-dev session logs). See [`docs/tmp.md`](docs/tmp.md) for the inventory and cleanup rules.

## Live dev invariant

- Desktop: Vite owns `http://localhost:1420/`. The live `[dev]` stream from `just dev` is the source of truth; a `:1421 failed` / `EADDRINUSE` line there means HMR is dead.
- Android: liveness = fresh WebView `connecting to …:1420` in `tmp/logcat.log` (`RustStdoutStderr`). USB/emulator localhost flows show `127.0.0.1`; Tauri 2.11 on Windows physical devices rewrites to the host LAN IP. `location.href` is **not** a signal — Tauri reports `http://tauri.localhost/` even when HMR is stale.
- While `tmp/_pids.json` exists and Vite owns `:1420`, do **not** run `cargo xtask android build`, `bun scripts/android-install.ts`, `cargo tauri build`, or any release build — each replaces the debug-dev APK and strands HMR.

## Per-turn during a live dev session

- Desktop: scan the `[dev]` stream for new error/panic lines. Android: diff `tmp/logcat.log` line counts. New failures → root-cause from `logs/*.log` (build) + `tmp/*.log` (dev session) + `adb logcat` + `git log -p`.
- Android JS edit must show `[vite] hmr update` in `tmp/android-dev.log`. Rust/native/config/capability edit requires `just dev stop && just android`.
- New `am_kill` / `am_proc_died` / `am_crash` for the app → inspect `tmp/logcat.log` around the timestamp and bisect against recent commits.

## Tauri workflow by change type

| Change                        | Touch                                                  | Verify                                              | Session action               |
| ----------------------------- | ------------------------------------------------------ | --------------------------------------------------- | ---------------------------- |
| Vue / TS / CSS                | `src/**`                                               | `bun run typecheck`; CDP eval                       | No restart; confirm HMR line |
| Rust command / IPC shape      | `commands/<domain>.rs`, `lib.rs`, `api.ts`, `types.ts` | Focused Rust test/check + typecheck                 | Android: restart bootstrap   |
| Rust native / long-running    | `src-tauri/src/**`                                     | Focused Rust test/check; inspect `RustStdoutStderr` | Android: restart bootstrap   |
| Tauri config / capability     | `tauri.conf.json`, `capabilities/*.json`               | Reproduce the exact invoke; check IPC errors        | Restart bootstrap            |
| Android scaffold / manifest   | `src-tauri/gen/android/**`                             | `bun scripts/android-install.ts` + manual probe     | Restart bootstrap            |
| Release / build orchestration | `xtask/**`, `justfile`, `scripts/install-*`            | Targeted command, then `just check`                 | Stop live dev first          |

## Decision table

| Need                          | Action                                                                                                     |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------- |
| Find code                     | Main-thread `Grep`/`Glob`, or `Explore` agent                                                              |
| Diagnose a failing log signal | Read `logs/*.log` (build) + `tmp/*.log` (dev session) + `adb logcat` + `git log -p`; bisect recent commits |
| Debug Tauri/WebView/IPC live  | Skill `tauri-v2`; CDP + logcat/`RustStdoutStderr`                                                          |
| Add/change Tauri command      | Main thread; sync handler + invoke + api.ts + types.ts + capabilities                                      |
| Edit project files            | Main thread (pre-commit hook is the gate)                                                                  |
| Install Android APK only      | `bun scripts/android-install.ts` (add `--force` to wipe + reinstall on sig mismatch)                       |
| Dev release                   | Windows: `just build` then `just release` (build → `releases/dev/`, publish rolling `dev` prerelease)      |
| Stable release                | `just release-stable`                                                                                      |

## Skills

Project-relevant skills live in `.agents/skills/` for pi and are mirrored in `.opencode/skills/` for opencode. The matching global copies have been removed to avoid duplicate or stale skill selection.

- `tauri-v2` — Tauri architecture, IPC, commands, capabilities, mobile, plugins, distribution.
- `tauri-debugging` — live desktop/Android/iOS WebView, CDP, logcat, Rust panic, IPC/capability triage.
- `rust-skills` — canonical Rust guidance; replaces redundant Rust best-practice/testing/async/pattern skills.
- `chrome-devtools` — CDP/live browser inspection for the Vite/WebView surface.
- `playwright-skill` — browser automation and end-to-end UI probes.
- `error-resolver` — systematic diagnosis for errors, stack traces, and unexpected behaviour.
- `verification-loop` — post-change verification and pre-PR quality gates.
- `docker-patterns` — Docker release/build troubleshooting, especially Linux `.deb` packaging.
- `improve-codebase-architecture` — architectural refactoring and module-depth reviews.
