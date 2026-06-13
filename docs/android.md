# Android

Dev-loop commands live in [`dev-loop.md`](dev-loop.md). This file: prerequisites, build/install, what `bootstrap` guarantees.

## Prerequisites

- Android Studio with SDK + NDK (version pinned in `justfile` `_android_ndk`)
- JDK 21
- Rust Android targets (`rustup target add aarch64-linux-android`, etc.)
- sherpa-onnx Android prebuilts are fetched automatically on first build into `.android-prebuilt/`

`bun scripts/doctor.ts` validates host prerequisites are reachable from the current shell.

## Build / install (no live session)

```bash
cargo xtask android build                # build the APK (aarch64 default)
cargo xtask android build --target armv7 # other targets: armv7 | x86_64 | i686
bun scripts/android-install.ts           # build + adb install -r
bun scripts/android-install.ts --force   # uninstall + reinstall (fixes signature mismatch)
```

The install script derives `ANDROID_HOME`/`NDK_HOME` from the standard SDK location on Windows and Linux, then forwards to `cargo xtask android build` followed by `adb`. The `.vscode/tasks.json` entries "android: build + install APK" and "android: build + reinstall APK (wipe data)" wrap it.

The keystore-properties path is regenerated per-host by `xtask/src/release/builders.rs::ensure_dev_keystore_properties` whenever the recorded `storeFile` is missing — same checkout signs APKs on Windows and Linux without manual edits.

## What `just android` guarantees

`cargo xtask android bootstrap usb` always stops any existing dev session and force-stops the app first, then brings up a fresh one. The bootstrap prints labelled `[stage N/7]` lines:

- **stage 0/7** — stops any previous session and force-stops the app for a clean restart.
- **stage 1/7** — preflight (`node_modules`, adb device target); writes `tmp/_platform`.
- **stage 2/7** — clears then tails focused logcat → `tmp/logcat.log` (baseline `*:S`, then `RustStdoutStderr:I`, `Tauri:I`, `chromium:W`, `AndroidRuntime:E`; `am_crash`, `am_proc_died`, `am_proc_start`, `am_kill` at `:V`). Also spawns `scripts/dev-vital.ts`.
- **stage 3a/7** — spawns a detached Vite dev server → `tmp/android-dev.{log,err.log}`. USB mode sets `TAURI_DEV_HOST=127.0.0.1` and `adb reverse tcp:1420`/`tcp:1421`; host mode detects the host LAN IP and sets `TAURI_DEV_HOST` to it.
- **stage 3b/7** — spawns `tauri android dev` (external-vite, frontend hook is an `echo` no-op) → `tmp/android-tauri.{log,err.log}`. Vite is owned by bootstrap so it survives the APK launch.
- **stage 4/7** — waits for Vite HMR ready on `:1420` (`Local:`/`Network:` + `:1420`); fast-fails on child death or signature mismatch.
- **stage 5/7** — waits for the cargo+gradle build → APK install/launch (any of `Info Opening …`, `Starting: Intent … wtranscriber`, `am_proc_start … wtranscriber`, or the `renderer error bridge installed` Rust log — covers cold cargo+NDK builds).
- **stage 6/7** — attaches WebView DevTools (≤90 s, event-driven: succeeds the moment the WebView devtools socket appears via `cat /proc/net/unix`), then probes Tauri IPC by invoking `system_info` over CDP (≤20 s, non-fatal — the attached DevTools socket already proves the session is live).
- **stage 7/7** — attaches lldb (best-effort; warns and continues on failure).

On an APK signature mismatch the bootstrap auto-recovers (uninstall + retry once). On success it writes `tmp/_pids.json` and prints `BOOTSTRAP OK …`.

`just android` runs xtask directly; there is no outer idle/max harness around Android dev bootstrap.

## Headless emulator

```bash
bun scripts/android-emu.ts        # cross-platform; bounded waits
```

Creates the AVD on first run, boots `-no-window -gpu swiftshader_indirect -accel on`. Each wait stage prints progress every 5 s.
