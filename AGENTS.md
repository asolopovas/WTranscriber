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
xtask/src/       bump / publish / release / android orchestration
scripts/         run.ts, parallel.ts, android-emu.ts, cdp.ts, lint-vue.ts, clean-temp.ts, doctor.ts, install-*.ps1, bootstrap-*
docs/            android · dev-loop · release · rust-build-speed
.agents/skills/  tauri (loaded by the Skill tool)
```

## Task contract

Every `just` recipe runs through `scripts/run.ts` (Bun + TypeScript): line-prefixed output, heartbeat after 10 s of silence, kill on idle (default 90 s) or hard timeout (default 600 s), final `OK in X.Ys` / `FAIL exit=N in X.Ys`. Long-running interactive recipes (`dev`, `dev-cpu`, `watch`) use `--idle 0 --max 0`; `just android` is finite (it bootstraps a detached session and exits) but uses `--idle 120 --max 2100` to absorb cold aarch64-android cargo + first-run gradle (10–30 min). Anything quiet >30 s during steady state is a bug.

## Commands

```
just dev               desktop HMR (Linux/Windows)
just android           Android USB/emu HMR session (idempotent)
just android-stop      stop the session
just android-emu       headless x86_64 emulator (cross-platform)
just check             parallel pre-release gate
just build             dev release build (host + Android + Linux .deb in parallel) → `releases/dev/`
just release           publish `releases/dev/` artifacts to the rolling gh `dev` prerelease
just release-stable    check + bump + build + publish (stable)
```

`just build` runs `cargo xtask release --dev`: builds the Windows NSIS installer (host or via the `windowsVm` entry in `release.config.json`), the Android APK, and the Linux `.deb` (Docker) in parallel into `releases/dev/`. Self-healing on transient Windows-VM failures uses the configured VM start/restart commands + 1 retry. `just release` is publish-only (`cargo xtask publish dev`); it never builds. See [`docs/release.md`](docs/release.md) for the failsafe + recovery flow.

`just check` runs **11 jobs** in parallel via `scripts/parallel.ts`: `fmt-check`, `clippy`, `clippy-xtask`, `typecheck`, `vue-lint`, `knip`, `rust-test`, `xtask-test`, `js-test`, `machete`, `audit`. First failure wins; all jobs complete. Sequential variants exist for targeted runs (`just lint`, `just test`, …). The same recipe runs in CI (`.github/workflows/check.yml`).

`just --list` for the rest.

## Conventions

- Rust edition 2024 (`LazyLock`, `let-else`); errors crossing JS use `error::Error` (`Serialize`).
- Tauri process split: Vue/WebView owns presentation; Rust owns filesystem, models, native, long work. Cross only via commands/events.
- `src/types.ts` mirrors Rust structs. Use aliases `@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`.
- New Tauri command = `commands/<domain>.rs` handler + `lib.rs` `invoke_handler![…]` (full path) + `api.ts` wrapper + `types.ts` mirror + `src-tauri/capabilities/default.json` permission if it touches a plugin/API.
- Capability permissions: least-privilege. If IPC fails, inspect console + `RustStdoutStderr` before widening.
- No comments in code. No `sleep` in scripts; poll with timeout.
- Conventional commits, simple British English.

## Pre-commit hook

`.githooks/pre-commit` is mandatory; `--no-verify` is forbidden (the sole exception is the release bump commit, which runs after `just check`). Auto-formats touched files (Rust via `cargo fmt` for `src-tauri`/`xtask`, TS/Vue/scripts/docs via `prettier --write`), re-stages, then gates on `bun run typecheck` for TS/Vue/scripts changes. Rust correctness (clippy, tests) is **not** in the hook — it lives in `just check` and CI. Run `just check` before opening a PR.

## Tauri 2.11 patches

In-tree workarounds for upstream issues until Tauri 2.12 lands:

- `src-tauri/gen/android/app/src/main/java/com/asolopovas/wtranscriber/generated/WryActivity.kt` carries inline `@Suppress("DEPRECATION")` annotations on the `onDestroy`/`onRestart` overrides so they don't fail `-Werror` Kotlin builds.
- `xtask/src/android/patch.rs::patch_plugin_consumer_rules` touches an empty `consumer-rules.pro` inside each plugin's `android/` dir referenced by `gen/android/tauri.settings.gradle` (covers `tauri-plugin-dialog`, `tauri-plugin-fs`). Wired into `prepare()` before every Android build.
- `src-tauri/build.rs::stub_windows_bundle_resources` touches zero-byte placeholders for the Windows bundle DLLs declared in `tauri.windows.conf.json`, so `tauri_build`'s resource validation passes during `just check` / dev builds on a fresh checkout. `install_cuda_dlls` then overwrites them with real binaries from `%APPDATA%` during release builds. Pre-bundle: verify file sizes before shipping a release.
- `src-tauri/build.rs::invalidate_stale_cmake_caches` wipes `target/{debug,release}/build/{whisper-rs-sys-*,sherpa-onnx-sys-*}` when `CMAKE_GENERATOR` changes vs the `target/.cmake-generator` sentinel.

Drop these once Tauri 2.12 publishes the fixed plugin gradle + activity migration.

## Windows host setup

`scripts/bootstrap-windows.ps1` (run by `just bootstrap`, which is a dependency of `just build`) installs/repairs: VS 2022 Build Tools, rustup (msvc), Bun, Node, NSIS, CMake, Ninja, LLVM/libclang, MSYS2, just, **CUDA Toolkit 12.x** (via `Nvidia.CUDA`), **cuDNN 9** (via `scripts/install-cudnn.ps1`), and **sherpa-onnx CUDA runtime** (via `scripts/install-sherpa-cuda.ps1`). Subsequent runs are idempotent. `just doctor` validates the same prerequisites are reachable from the current shell.

## Scratch artefacts

- `logs/<tag>.log` — per-tag build logs written by `scripts/run.ts`. **Wiped on every `just build*`.**
- `tmp/` — dev-loop source of truth (PIDs, logcat, android-dev session logs, agent reports). See [`docs/tmp.md`](docs/tmp.md) for the inventory and cleanup rules.

## Live dev invariant

- Desktop: Vite owns `http://localhost:1420/`. The live `[dev]` stream from `just dev` is the source of truth; a `:1421 failed` / `EADDRINUSE` line there means HMR is dead.
- Android: liveness = fresh `connecting to 127.0.0.1:1420` in `tmp/logcat.log` (`RustStdoutStderr`). `location.href` is **not** a signal — Tauri reports `http://tauri.localhost/` even when HMR is stale.
- While `tmp/_pids.json` exists and Vite owns `:1420`, do **not** run `just android-install`, `just android-build`, `cargo tauri build`, or any release build — each replaces the debug-dev APK and strands HMR.

## Per-turn during a live dev session

- Desktop: scan the `[dev]` stream for new error/panic lines. Android: diff `tmp/logcat.log` line counts. New failures → root-cause from `logs/*.log` (build) + `tmp/*.log` (dev session) + `adb logcat` + `git log -p`.
- Android JS edit must show `[vite] hmr update` in `tmp/android-dev.log`. Rust/native/config/capability edit requires `just android-stop && just android`.
- New `am_kill` / `am_proc_died` / `am_crash` for the app → inspect `tmp/logcat.log` around the timestamp and bisect against recent commits.

## Tauri workflow by change type

| Change                        | Touch                                                  | Verify                                              | Session action               |
| ----------------------------- | ------------------------------------------------------ | --------------------------------------------------- | ---------------------------- |
| Vue / TS / CSS                | `src/**`                                               | `bun run typecheck`; CDP eval                       | No restart; confirm HMR line |
| Rust command / IPC shape      | `commands/<domain>.rs`, `lib.rs`, `api.ts`, `types.ts` | Focused Rust test/check + typecheck                 | Android: restart bootstrap   |
| Rust native / long-running    | `src-tauri/src/**`                                     | Focused Rust test/check; inspect `RustStdoutStderr` | Android: restart bootstrap   |
| Tauri config / capability     | `tauri.conf.json`, `capabilities/*.json`               | Reproduce the exact invoke; check IPC errors        | Restart bootstrap            |
| Android scaffold / manifest   | `src-tauri/gen/android/**`                             | `just android-smoke` on a connected device          | Restart bootstrap            |
| Release / build orchestration | `xtask/**`, `justfile`, `scripts/install-*`            | Targeted command, then `just check`                 | Stop live dev first          |

## Decision table

| Need                          | Action                                                                                                     |
| ----------------------------- | ---------------------------------------------------------------------------------------------------------- |
| Find code                     | Main-thread `Grep`/`Glob`, or `Explore` agent                                                              |
| Diagnose a failing log signal | Read `logs/*.log` (build) + `tmp/*.log` (dev session) + `adb logcat` + `git log -p`; bisect recent commits |
| Debug Tauri/WebView/IPC live  | Skill `tauri` (debugging section); CDP + logcat/`RustStdoutStderr`                                         |
| Add/change Tauri command      | Main thread; sync handler + invoke + api.ts + types.ts + capabilities                                      |
| Edit project files            | Main thread (pre-commit hook is the gate)                                                                  |
| Install + smoke-test          | `just android-install` + `just android-smoke` (or host installer build)                                    |
| Dev release                   | `just build` then `just release` (build → `releases/dev/`, publish rolling `dev` prerelease)               |
| Stable release                | `just release-stable`                                                                                      |

## Skills

- `tauri` — load before architectural / IPC / capability / mobile / distribution changes, and for WebView/CDP/logcat debugging.
