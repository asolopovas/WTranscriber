# Android

## Prerequisites

SDK/NDK from Android Studio, JDK 21, and Rust targets for the selected ABI. NDK `27.2.12479018` is pinned in `justfile`'s `_android_ndk`. Sherpa prebuilts download into `.android-prebuilt/` on first build. `just doctor` checks prerequisites.

## APK build/install

Run outside a live HMR session:

```bash
cargo xtask android build
cargo xtask android build --target armv7
bun scripts/android-install.ts
```

Targets: `aarch64` (default), `armv7`, `i686`, `x86_64`. The install script derives SDK/NDK paths for Windows/Linux and uses `adb install -r`. Signature mismatch fails unless `--force` is chosen; force uninstalls and wipes app data. VS Code wraps both install modes.

`ensure_dev_keystore_properties` in `xtask/src/release/builders.rs` regenerates missing host-specific keystore paths for Android and release builds.

## Bootstrap contract

`just android` invokes `cargo xtask android bootstrap usb` directly, without the run harness watchdog. Stages:

| Stage | Work                                                                                                |
| ----- | --------------------------------------------------------------------------------------------------- |
| 0     | Stop previous session and force-stop app                                                            |
| 1     | Check node_modules/device; write `tmp/_platform`                                                    |
| 2     | Clear/tail focused logcat; start `scripts/dev-vital.ts`                                             |
| 3a    | Start bootstrap-owned Vite; configure USB reverse or LAN host                                       |
| 3b    | Start `tauri android dev` with external Vite                                                        |
| 4     | Await HMR on 1420; fail on child death/signature mismatch                                           |
| 5     | Await Cargo/Gradle, APK installation, and launch                                                    |
| 6     | Await WebView socket in `/proc/net/unix` (90 s); probe `system_info` IPC over CDP (20 s, non-fatal) |
| 7     | Best-effort lldb attachment                                                                         |

Success writes `tmp/_pids.json`, prints `BOOTSTRAP OK`, and exposes CDP on 9222. Signature mismatch triggers one uninstall/retry, wiping app data. Logcat captures RustStdoutStderr/Tauri info, chromium warnings, AndroidRuntime errors, and app process/crash events.

[dev-loop.md](dev-loop.md#live-session-signals) owns liveness/restart rules; [tmp.md](tmp.md) owns output files.

## WebView inspection

```bash
bun scripts/cdp.ts 'document.title'
```

The helper uses `dev.config.ts`'s CDP host/port, accepts `CDP_HOST`/`CDP_PORT` overrides, and selects the first page target with a WebSocket debugger URL. It evaluates with awaitPromise/returnByValue and a 15 s timeout; verify the selected target when several exist. Use `http://127.0.0.1:9222/json` to inspect forwarded targets after restart.

A stale LAN HMR URL after switching to USB requires refetching the page from the current server; do not replace the APK during HMR. Native crash symbolisation uses unstripped libraries under `target/<triple>/{debug,release}/deps/`, not APK-stripped copies.

## Emulator

`bun scripts/android-emu.ts` creates the AVD if needed and boots headlessly with no audio, SwiftShader, and acceleration. Waits are bounded with progress every 5 s.

Source: `xtask/src/android/`, `scripts/android-install.ts`, `scripts/android-emu.ts`, `scripts/cdp.ts`.
