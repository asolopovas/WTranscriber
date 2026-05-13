# Android

Dev-loop commands live in [`dev-loop.md`](dev-loop.md). This file: prerequisites, build/install, what `bootstrap` guarantees.

## Prerequisites

- Android Studio with SDK + NDK (version pinned in `justfile` `_android_ndk` — currently `27.2.12479018`)
- JDK 21
- Rust Android targets (`rustup target add aarch64-linux-android`, etc.)
- sherpa-onnx Android prebuilts (auto-fetched on first build into `.android-prebuilt/`; manual: `cargo xtask android prebuilts`)

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

`cargo xtask android bootstrap usb`:

1. No-ops if a healthy session exists.
2. Validates adb device, writes `tmp/_platform`.
3. Clears + tails logcat → `tmp/logcat.log` (W+, RustStdoutStderr/Tauri/chromium; `am_crash`, `am_proc_died`, `am_proc_start`, `am_kill` raised to V).
4. Configures `adb reverse tcp:1420` and `tcp:1421`.
5. Spawns `tauri android dev --no-watch` detached → `tmp/android-dev.{log,err.log}`.
6. Waits for Vite ready event (`Local:` + `:1420` line in `tmp/android-dev.log`, ≤90 s; fast-fails on child death or signature mismatch).
7. Waits for cargo+gradle build → APK install/launch (any of `Info Opening`, `Info Installing`, `Performing Streamed Install`, `Starting: Intent … wtranscriber`, or `am_proc_start … wtranscriber`, ≤1800 s — covers cold cargo+NDK builds).
8. Waits for WebView event (`connecting to … :1420` in `tmp/logcat.log`, ≤90 s).
9. Forwards CDP to `127.0.0.1:9222` (event-driven: succeeds the moment the WebView devtools socket appears); probes Tauri IPC (`appVersion`, `systemInfo`, `loadConfig`) for ≤20 s — non-fatal, since the WebView-connected event already proves the session is live.
10. Auto-recovers signature mismatch (uninstall + retry once).
11. Writes `tmp/_pids.json` and prints `BOOTSTRAP OK …`.

Outer harness budget: `--idle 120 --max 2100` (cold aarch64-android cargo + first-run gradle commonly takes 10–30 min; warm builds finish in <30 s).

## Headless emulator

```bash
bun scripts/android-emu.ts        # cross-platform; bounded waits
```

Creates the AVD on first run, boots `-no-window -gpu swiftshader_indirect -accel on`. Each wait stage prints progress every 5 s.
