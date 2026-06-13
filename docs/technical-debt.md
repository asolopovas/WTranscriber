# Technical debt

Track temporary patches, known cleanup work, and removal triggers here. Debt should be specific enough for an agent to retire it without external memory.

## Tauri 2.11 patches

Drop these once Tauri 2.12 publishes the fixed plugin Gradle and activity migration behaviour, then verify Android build/install.

- `src-tauri/gen/android/app/src/main/java/com/asolopovas/wtranscriber/generated/WryActivity.kt` carries inline `@Suppress("DEPRECATION")` annotations on the `packageManager.getPackageInfo(...)` calls in the WebView-version getter so they do not fail `-Werror` Kotlin builds.
- `xtask/src/android/patch.rs::patch_plugin_consumer_rules` touches an empty `consumer-rules.pro` inside each plugin `android/` dir referenced by `gen/android/tauri.settings.gradle`; currently covers `tauri-plugin-dialog` and `tauri-plugin-fs`. It runs in `prepare()` before every Android build.
- `src-tauri/build.rs::stub_windows_bundle_resources` touches the Windows bundle placeholder needed by `tauri_build` resource validation during `just check` / dev builds on a fresh checkout. `install_cuda_dlls` copies real CUDA DLLs from `%APPDATA%` during release builds. Pre-bundle, verify file sizes before shipping a release.
- `src-tauri/build.rs` warns when `CMAKE_GENERATOR` changes; `xtask/src/check.rs` owns the cache wipe for `target/{debug,release}/build/{whisper-rs-sys-*,sherpa-onnx-sys-*}` using the `target/.cmake-generator` sentinel.
- `xtask/src/release/builders.rs::ensure_dev_keystore_properties` regenerates `src-tauri/gen/android/keystore.properties` whenever the recorded `storeFile` is missing on the current host. It is called from both `cargo xtask android build` and the release matrix so the same checkout signs APKs on Windows and Linux.

## Vendored symphonia-format-isomp4

`src-tauri/vendor/symphonia-format-isomp4` is upstream 0.5.5 with one change: `SLDescriptor::read` tolerates a non-MP4 SL config descriptor (skips it with a warning) instead of erroring. WhatsApp voice notes (`AUD-*-WA*.m4a`) set `predefined != 2`, which broke every ffmpeg-less decode path — all of Android, plus the desktop silero-langid probe (`audio/decode.rs`). Wired via `[patch.crates-io]` in `src-tauri/Cargo.toml`. Drop the vendor dir and the patch entry when symphonia ships a release that accepts these files.
