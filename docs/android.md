# Android

Dev-loop commands: [`dev-loop.md`](dev-loop.md). This file: prerequisites, build/install, bootstrap stages.

## Prerequisites

- Android Studio with SDK + NDK (version `27.2.12479018`, pinned in `justfile` `_android_ndk`)
- JDK 21
- Rust Android targets (`rustup target add aarch64-linux-android`, etc.)
- sherpa-onnx Android prebuilts fetch automatically on first build into `.android-prebuilt/`

`bun scripts/doctor.ts` (or `just doctor`) validates host prerequisites.

## Build / install (no live session)

```bash
cargo xtask android build                # build the APK (aarch64 default)
cargo xtask android build --target armv7 # targets: aarch64 | armv7 | i686 | x86_64
bun scripts/android-install.ts           # build + adb install -r
bun scripts/android-install.ts --force   # on signature mismatch: uninstall (wipes data) + reinstall
```

`android-install.ts` derives `ANDROID_HOME`/`NDK_HOME` from the standard SDK location (Windows/Linux), runs `cargo xtask android build`, then `adb install -r`. Without `--force` a signature mismatch fails with instructions. `.vscode/tasks.json` wraps it as "android: build + install APK" and "android: build + reinstall APK (wipe data)".

`xtask/src/release/builders.rs::ensure_dev_keystore_properties` regenerates the keystore-properties path per-host when the recorded `storeFile` is missing, so the same checkout signs APKs on Windows and Linux.

## What `just android` guarantees

`just android` runs `cargo xtask android bootstrap usb` directly (no idle/max harness). It stops any existing session and force-stops the app first, then brings up a fresh one. Stages are labelled `[stage N/7]`:

- **0** — stop previous session, force-stop app.
- **1** — preflight (`node_modules`, adb device); writes `tmp/_platform`.
- **2** — clears then tails focused logcat → `tmp/logcat.log` (`*:S` baseline + `RustStdoutStderr:I`, `Tauri:I`, `chromium:W`, `AndroidRuntime:E`; `am_crash`/`am_proc_died`/`am_proc_start`/`am_kill` at `:V`). Spawns `scripts/dev-vital.ts`.
- **3a** — Vite dev server → `tmp/android-dev.{log,err.log}`. USB sets `TAURI_DEV_HOST=127.0.0.1` + `adb reverse tcp:1420`/`tcp:1421`; host mode detects LAN IP. Vite is bootstrap-owned so it survives the APK launch.
- **3b** — `tauri android dev` (external-vite) → `tmp/android-tauri.{log,err.log}`.
- **4** — waits for Vite HMR on `:1420`; fast-fails on child death or signature mismatch.
- **5** — waits for cargo+gradle build → APK install/launch.
- **6** — attaches WebView DevTools (≤90 s, succeeds when the devtools socket appears via `/proc/net/unix`), then probes Tauri IPC via `system_info` over CDP (≤20 s, non-fatal). **This is the liveness signal.**
- **7** — attaches lldb (best-effort).

On APK signature mismatch the bootstrap auto-recovers (uninstall + retry once). On success it writes `tmp/_pids.json` and prints `BOOTSTRAP OK …` (CDP on `tcp:9222`).

## Headless emulator

```bash
bun scripts/android-emu.ts        # cross-platform; bounded waits
```

Creates the AVD on first run, boots `-no-window -no-audio -gpu swiftshader_indirect -accel on`. Each wait stage prints progress every 5 s.
