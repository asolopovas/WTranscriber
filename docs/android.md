# Android

Dev-loop commands live in [`dev-loop.md`](dev-loop.md). This file: prerequisites, build/install, what `bootstrap` guarantees.

## Prerequisites

- Android Studio with SDK + NDK (version pinned in `justfile` `_android_ndk` — currently `27.2.12479018`)
- JDK 21
- `just android-targets` — Rust Android targets
- `just android-prebuilts` — sherpa-onnx Android prebuilts

Run `just android-doctor` for an authoritative toolchain check; it reads the
actual NDK path the build will use and fails loudly on drift.

## Build / install

```bash
just android-build              # build APK
just android-install            # build + install
just android-install-fresh      # uninstall + install (fixes signature mismatch)
just android-doctor             # toolchain sanity
just android-cli                # build wt CLI for Android
just android-cli-push           # push to /data/local/tmp
just android-cli-run -- --help  # run on device
```

Pass `target=<aarch64|armv7|x86_64|i686>`; default `aarch64`. Do not run install/build while a dev session is live.

## What `just android` guarantees

`cargo xtask android bootstrap usb`:

1. No-ops if a healthy session exists.
2. Validates adb device, writes `tmp/_platform`.
3. Clears + tails logcat → `tmp/logcat.log` (W+, RustStdoutStderr/Tauri/chromium/am\_\*).
4. Configures `adb reverse tcp:1420` and `tcp:1421`.
5. Spawns `tauri android dev --no-watch` detached → `tmp/android-dev.{log,err.log}`.
6. Waits for Vite ready event (`Local:` + `:1420` line in `tmp/android-dev.log`, ≤90 s; fast-fails on child death or signature mismatch).
7. Waits for cargo+gradle build → APK install/launch (`Info Opening`/`Finished … profile`/`am_proc_start` event, ≤1800 s — covers cold cargo+NDK builds).
8. Waits for WebView event (`connecting to … :1420` in `tmp/logcat.log`, ≤90 s).
9. Forwards CDP to `127.0.0.1:9222` (event-driven: succeeds the moment the WebView devtools socket appears); probes Tauri IPC (`appVersion`, `systemInfo`, `loadConfig`) for ≤20 s — non-fatal, since the WebView-connected event already proves the session is live.
10. Auto-recovers signature mismatch (uninstall + retry once).
11. Writes `tmp/_pids.json` and prints `BOOTSTRAP OK …`.

Outer harness budget: `--idle 120 --max 2100` (cold aarch64-android cargo + first-run gradle commonly takes 10–30 min; warm builds finish in <30 s).

`just android-status-json` reports: `sessionHealthy`, `viteAlive`, `reverse1420`, `reverse1421`, `cdpForward`, `apiResponsive`, log ages, last HMR update, last crash signal.

## Headless emulator

```bash
just android-emu       # cross-platform; bounded waits
just android-emu-stop
```

Backed by `scripts/android-emu.ts`. Creates the AVD on first run, boots `-no-window -gpu swiftshader_indirect -accel on`. Each wait stage prints progress every 5 s.
