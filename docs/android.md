# Android

Dev-loop commands live in [`dev-loop.md`](dev-loop.md). This file: prerequisites, build/install, what `bootstrap` guarantees.

## Prerequisites

- Android Studio with SDK + NDK `27.2.12479018`
- JDK 21
- `just android-targets` — Rust Android targets
- `just android-prebuilts` — sherpa-onnx Android prebuilts

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
6. Waits for Vite `:1420` (≤180 s, fast-fails on child death or signature mismatch).
7. Waits for WebView `connecting to … :1420` (≤120 s).
8. Forwards CDP to `127.0.0.1:9222`; probes Tauri IPC (`appVersion`, `systemInfo`, `loadConfig`).
9. Auto-recovers signature mismatch (uninstall + retry once).
10. Writes `tmp/_pids.json` and prints `BOOTSTRAP OK …`.

`just android-status-json` reports: `sessionHealthy`, `viteAlive`, `reverse1420`, `reverse1421`, `cdpForward`, `apiResponsive`, log ages, last HMR update, last crash signal.

## Headless emulator

```bash
just android-emu       # cross-platform; bounded waits
just android-emu-stop
```

Backed by `scripts/android-emu.mjs`. Creates the AVD on first run, boots `-no-window -gpu swiftshader_indirect -accel on`. Each wait stage prints progress every 5 s.
