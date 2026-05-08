# Android build

## Status: **WORKING** (debug APK builds and packages)

```
src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

Verified: 129 MB APK containing `lib/arm64-v8a/{libwtranscriber_lib.so,
libsherpa-onnx-c-api.so, libsherpa-onnx-cxx-api.so, libsherpa-onnx-jni.so,
libonnxruntime.so, libc++_shared.so}`.

> ⚠ The APK installs and launches, but transcription at runtime depends on
> sidecar binaries (`sherpa-onnx-offline`, `llama-cli`, NeMo Sortformer)
> that don't run on Android's app sandbox. The native sherpa-onnx libraries
> are bundled in the APK; wiring them in-process via the `sherpa-onnx`
> Rust crate (instead of `tauri-plugin-shell`) is a separate task.

## Prerequisites

- Android Studio with SDK + NDK r27.x (currently `27.2.12479018`).
- JDK 21 with `JAVA_HOME` set.
- Rust 1.85+ with the four Android targets (`just android-targets`).

`justfile` auto-detects `ANDROID_HOME` at `%LOCALAPPDATA%\Android\Sdk` and
`NDK_HOME` at `$ANDROID_HOME\ndk\27.2.12479018`. Override via env vars.

## How linking works

`sherpa-onnx-sys` 1.13 has no Android prebuilts in its release matrix, so
the crate's auto-download fails for `*-linux-android` targets. Workaround:

1. `scripts/install-android-prebuilts.ps1` downloads the official
   `sherpa-onnx-v$VERSION-android.tar.bz2` from the k2-fsa releases page
   into `.android-prebuilt/jniLibs/<abi>/`. Version comes from
   `src-tauri/sherpa-version.txt`.
2. `scripts/android-build.ps1` exports `SHERPA_ONNX_LIB_DIR` pointing at
   the per-ABI prebuilt directory. The sys crate's `resolve_lib_dir`
   short-circuits to that path and emits the right `-L` / `-l` flags.
3. The same `.so` files are also placed in
   `src-tauri/gen/android/app/src/main/jniLibs/<abi>/` so they're packaged
   into the APK alongside `libwtranscriber_lib.so`.
4. `ort` (ONNX Runtime) auto-downloads its own Android prebuilts via
   `pyke.io` cache — handled transparently.
5. NDK clang / linker are wired through
   `CC_<triple>` / `CXX_<triple>` / `AR_<triple>` /
   `CARGO_TARGET_<TRIPLE>_LINKER` env vars.

## Recipes

```
just android-doctor          show resolved SDK / NDK / installed Rust targets
just android-targets         rustup add the four Android triples
just android-prebuilts       download + extract sherpa-onnx Android .so files
just android-init            tauri android init (one-time scaffold)
just android-dev             run on connected device / emulator (USB, HMR via adb reverse)
just android-dev-host        same, but bind dev server to LAN IP (Wi-Fi / non-USB)
just android-build           release APK (default target=aarch64)
just android-build-debug     debug APK
```

`target` ∈ `aarch64`, `armv7`, `i686`, `x86_64`. Currently only `aarch64`
has `.so` files staged in the APK's `jniLibs` (others can be added with
`cp -r .android-prebuilt/jniLibs/<abi> src-tauri/gen/android/app/src/main/jniLibs/`).

## Live UI dev (HMR, no rebuild/reinstall)

`tauri android dev` keeps the WebView pointed at the Vite dev server, so
edits to `src/**` (Vue / TS / CSS) hot-reload in-place. Only Rust changes
trigger a native rebuild + reinstall.

`vite.config.ts` already reads `TAURI_DEV_HOST`. `src-tauri/tauri.conf.json`
uses `devUrl: http://localhost:1420`. Two transport modes:

| Mode                  | Recipe                  | Transport                                                                                                                                    |
| --------------------- | ----------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| USB device / emulator | `just android-dev`      | Tauri CLI runs `adb reverse tcp:1420` + HMR port; WebView hits `localhost:1420`.                                                             |
| Wi-Fi / no USB        | `just android-dev-host` | Adds `--host`; Tauri sets `TAURI_DEV_HOST=<LAN IP>`, Vite binds to it, HMR over `ws://<LAN>:1421`. Phone + host must share the same network. |

First run installs the debug APK once. Subsequent UI edits stream over the
HMR socket — no `adb install`, no Gradle, no Tauri rebuild.

For a tight design loop:

1. `just android-dev` (leave running).
2. Edit Vue / CSS — changes appear instantly on device.
3. Open `chrome://inspect` (see `docs/tauri-debug.md`) for live DOM /
   console / network on the device WebView.
4. Use `node scripts/cdp.mjs "<expr>"` to poke component state without
   touching the device.

Gotchas:

- Rust edits (`src-tauri/**`) still need a rebuild. Use `just android-dev`
  in one terminal; on save Tauri rebuilds the `.so` and reinstalls.
- `--host` requires the firewall to allow inbound TCP `1420` + `1421` on
  the dev machine.
- If HMR stalls, check `adb logcat -s chromium:V Console:V` for WS
  connect errors, then verify the host can reach `http://<LAN>:1420` from
  a phone browser.

## Remaining work for a runtime-functional app

The build pipeline is solved. To make transcription actually work on
Android, the runtime must be ported off `tauri-plugin-shell` sidecars:

- Replace `sherpa-onnx-offline` exec with the in-process `sherpa-onnx`
  Rust crate (already linked via the Android prebuilts shipped in the
  APK).
- `llama-cli` and NeMo Sortformer have no Android equivalents in this
  repo; gate behind `cfg(not(target_os = "android"))` and either skip
  diarization on mobile v1 or integrate `llama.cpp` JNI bindings.
- Disable the `cuda` feature for Android (CUDA is desktop-only).
