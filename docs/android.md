# Android build

Module of [`AGENTS.md`](../AGENTS.md). Covers Android build pipeline, linking, and HMR-based UI dev. For runtime debugging load the global skill `tauri-debugging`.

## Status

Debug APK builds and packages:

```
src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

129 MB, contains `lib/arm64-v8a/{libwtranscriber_lib.so, libsherpa-onnx-c-api.so, libsherpa-onnx-cxx-api.so, libsherpa-onnx-jni.so, libonnxruntime.so, libc++_shared.so}`.

APK installs and launches. Transcription does not work at runtime: it depends on sidecar binaries (`sherpa-onnx-offline`, `llama-cli`, NeMo Sortformer) that don't run in Android's app sandbox. Native sherpa-onnx libs are bundled; wiring them in-process via the `sherpa-onnx` Rust crate (instead of `tauri-plugin-shell`) is a separate task.

## Prerequisites

- Android Studio with SDK + NDK r27.x (currently `27.2.12479018`).
- JDK 21, `JAVA_HOME` set.
- Rust 1.85+ with the four Android targets (`just android-targets`).

`justfile` auto-detects `ANDROID_HOME` at `%LOCALAPPDATA%\Android\Sdk` and `NDK_HOME` at `$ANDROID_HOME\ndk\27.2.12479018`. Override via env vars.

## Linking

`sherpa-onnx-sys` 1.13 has no Android prebuilts, so the crate's auto-download fails for `*-linux-android` targets. Workaround:

1. `scripts/install-android-prebuilts.ps1` downloads `sherpa-onnx-v$VERSION-android.tar.bz2` from the k2-fsa releases page into `.android-prebuilt/jniLibs/<abi>/`. Version comes from `src-tauri/sherpa-version.txt`.
2. `scripts/android-build.ps1` exports `SHERPA_ONNX_LIB_DIR` per ABI. The sys crate's `resolve_lib_dir` short-circuits to that path.
3. The same `.so` files are copied to `src-tauri/gen/android/app/src/main/jniLibs/<abi>/` for APK packaging.
4. `ort` (ONNX Runtime) auto-downloads its own Android prebuilts via `pyke.io` cache.
5. NDK clang/linker wired through `CC_<triple>`, `CXX_<triple>`, `AR_<triple>`, `CARGO_TARGET_<TRIPLE>_LINKER`.

## Commands

```
just android-doctor          show resolved SDK / NDK / Rust targets
just android-targets         rustup add the four Android triples
just android-prebuilts       download + extract sherpa-onnx Android .so
just android-init            tauri android init (one-time scaffold)
just android-dev             run on device/emulator (USB, HMR via adb reverse) — ABI auto-detected
just android-dev-host        same, dev server on LAN IP (Wi-Fi) — ABI auto-detected
just android-build           release APK (default target=aarch64)
just android-build-debug     debug APK
```

`target` (build/install/cli/doctor) is `aarch64`, `armv7`, `i686`, or `x86_64`. Only `aarch64` has `.so` files staged. Add others with `cp -r .android-prebuilt/jniLibs/<abi> src-tauri/gen/android/app/src/main/jniLibs/`.

`android-dev` does **not** take a `--target`: the upstream `tauri android dev` CLI (`tauri-cli` v2, `mobile/android/dev.rs`) auto-derives the ABI from the connected device's `ro.product.cpu.abi`. Our xtask reads the same property over `adb` to stage matching prebuilts/jniLibs before launching. Pass an optional device serial when multiple are attached: `just android-dev <serial>`.

## Live UI dev (HMR)

`tauri android dev` keeps the WebView pointed at the Vite dev server. Edits to `src/**` (Vue/TS/CSS) hot-reload over WebSocket — no rebuild, no reinstall, no app relaunch.

`vite.config.ts` reads `TAURI_DEV_HOST`. `src-tauri/tauri.conf.json` uses `devUrl: http://localhost:1420`.

| Mode           | Recipe                  | Transport                                                            |
| -------------- | ----------------------- | -------------------------------------------------------------------- |
| USB / emulator | `just android-dev`      | `adb reverse tcp:1420` + HMR port; WebView hits `localhost:1420`.    |
| Wi-Fi / no USB | `just android-dev-host` | `--host` sets `TAURI_DEV_HOST=<LAN IP>`, HMR over `ws://<LAN>:1421`. |

First run installs the debug APK. Subsequent UI edits stream over HMR.

### Frontend / backend separation

`android-dev` passes `--no-watch` to `tauri android dev`, so Tauri's Rust file watcher is **off** by default. This is deliberate:

- Frontend edits (`src/**`) → instant HMR push to the running app. The dev session never restarts.
- Backend or native Android edits (`src-tauri/**`, `gen/android/**` — kotlin, res XML, AndroidManifest, gradle) → no auto-rebuild. From a second terminal:
  ```
  just android-install
  ```
  Rebuilds Rust, signs, `adb install -r` (preserves app data). Dev session stays alive; HMR reattaches on WebView reload. **Never** run `just android-build` (release) or invoke `wt-installer` mid-session — both replace the debug-dev APK with a bundled-asset APK and silently break HMR; the on-device frontend goes stale while config files remain untouched.

Opt back into Tauri's auto-rebuild-on-Rust-save with `cargo xtask android dev --watch` if you really want the old behavior.

### Live DOM / CSS inspection (no screenshots)

With the dev session running, attach Chrome DevTools Protocol once:

```
just android-debug-attach           # adb forward tcp:9222 → webview_devtools_remote_<pid>
```

Then evaluate any JS in the live WebView from the host:

```
node scripts/cdp.mjs "document.querySelector('header').getBoundingClientRect()"
node scripts/cdp.mjs "getComputedStyle(document.querySelector('header')).height"
node scripts/cdp.mjs "document.querySelector('header').className"
```

This is the preferred diagnostic path — exact computed values, no PNG round-trip. Use `adb exec-out screencap -p > tmp/screen.png` only when a _visual_ judgment is required (font rendering, animation frame, overall composition).

### Design loop

1. `just android-dev` (leave running, ABI auto-detected, frontend-only HMR).
2. Edit Vue/CSS → changes appear instantly on device.
3. `node scripts/cdp.mjs "<expr>"` for sizing/style verification.
4. `chrome://inspect` for full DevTools (DOM tree, network, console, breakpoints) — see skill `tauri-debugging`.
5. Backend change? `just android-install` in another terminal; HMR session keeps running.

Gotchas:

- `--host` requires firewall to allow inbound TCP 1420 and 1421.
- HMR stalls: check `adb logcat -s chromium:V Console:V` for WS errors; verify host reachable at `http://<LAN>:1420` from phone browser.
- The empty conhost window that appears when launching dev is Tauri's `beforeDevCommand` (`bun run dev`) spawned with `CREATE_NEW_PROCESS_GROUP` on Windows. Harmless. Closing it kills Vite.

## Runtime work remaining

Port off `tauri-plugin-shell` sidecars:

- Replace `sherpa-onnx-offline` exec with the in-process `sherpa-onnx` Rust crate (Android prebuilts already shipped).
- `llama-cli` and NeMo Sortformer have no Android equivalents; gate behind `cfg(not(target_os = "android"))` and either skip diarization on mobile v1 or integrate `llama.cpp` JNI bindings.
- Disable the `cuda` feature for Android.
