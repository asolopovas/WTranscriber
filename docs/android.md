# Android workflow

Module of [`AGENTS.md`](../AGENTS.md). The Android dev loop is automated through `just` and `cargo xtask`; avoid manual adb/Vite/CDP setup.

## Prerequisites

- Android Studio with SDK + NDK `27.2.12479018`
- JDK 21
- Rust Android targets: `just android-targets`
- Android prebuilts: `just android-prebuilts`

## Dev commands

```bash
just android-bootstrap usb       # detached USB/emulator HMR, adb reverse, logcat, CDP, IPC probe
just android-bootstrap host      # detached Wi-Fi/LAN HMR
just android-status              # bounded health check
just android-status-json         # machine-readable health
just android-smoke               # fail-fast adb + Vite + CDP + Tauri IPC probe
just android-debug-eval "..."    # evaluate JS in the live WebView
just android-stop                # stop detached session and forwards
```

Pass a device serial as the final argument when multiple adb devices are attached, for example `just android-bootstrap usb R5CXB2PGC2H`.

## Build commands

```bash
just android-doctor
just android-build
just android-install-fresh
just android-cli-run -- --help
```

Do not run install/build/release tasks while the HMR session is live (`tmp/_pids.json` exists and Vite owns `:1420`). Stop and bootstrap again instead.

## What bootstrap guarantees

`cargo xtask android bootstrap` validates the adb device, clears and captures logcat, configures USB reverse ports, starts `tauri android dev --no-watch`, waits for Vite `:1420`, waits for the WebView to connect, forwards CDP to `127.0.0.1:9222`, and probes Tauri IPC via `appVersion`, `systemInfo`, and `loadConfig`.

Health lives in `just android-status-json`: `sessionHealthy`, `viteAlive`, `reverse1420`, `reverse1421`, `cdpForward`, `apiResponsive`, log ages, latest HMR update, and app-specific crash signal.

## HMR rule

Frontend edits (`src/**`) hot-reload. Backend/native/config edits require:

```bash
just android-stop
just android-bootstrap usb
```

`location.href` is always `http://tauri.localhost/` on Android; use `just android-status` and `tmp/android-dev.log` for liveness.
