# Dev loop

Use the automated Android tasks; do not recreate bootstrap steps by hand.

## Android

```bash
just android-bootstrap usb     # USB / emulator
just android-bootstrap host    # Wi-Fi / LAN
just android-status            # bounded health check: adb, reverse, Vite, CDP, IPC
just android-smoke             # fail-fast end-to-end probe
just android-stop              # stop detached dev session and forwards
just android-debug-eval "document.title"
```

`android-bootstrap` is implemented in `cargo xtask android bootstrap`. It validates the selected adb device, starts logcat, configures `adb reverse` for USB mode, starts Tauri Android dev, waits for Vite and the WebView connection, attaches CDP, and probes Tauri IPC through the live app.

Frontend edits (`src/**`) HMR in place. Backend/native/config edits require `just android-stop && just android-bootstrap usb` so the debug-dev APK and Vite dev URL stay paired.

Never run `just android-install`, `just android-build`, `cargo tauri build`, or release installers while `tmp/_pids.json` exists and Vite owns `:1420`.

## Signals

- Health: `just android-status-json`
- Live WebView eval: `just android-debug-eval "<expr>"`
- HMR proof after JS/CSS edits: `[vite] hmr update /src/...` in `tmp/android-dev.log`
- Crash/OOM proof: app-specific `am_kill`, `am_proc_died`, or `am_crash` in `tmp/logcat.log`

`location.href` is not a health signal on Android; Tauri reports `http://tauri.localhost/` even when HMR is stale.
