# Technical debt

Track temporary patches, known cleanup work, and removal triggers here. Debt should be specific enough for an agent to retire it without external memory.

## Tauri 2.11 patches

Drop these once Tauri 2.12 publishes the fixed plugin Gradle and activity migration behaviour, then verify Android build/install.

- `src-tauri/gen/android/app/src/main/java/com/asolopovas/wtranscriber/generated/WryActivity.kt` carries inline `@Suppress("DEPRECATION")` annotations on the `onDestroy`/`onRestart` overrides so they do not fail `-Werror` Kotlin builds.
- `xtask/src/android/patch.rs::patch_plugin_consumer_rules` touches an empty `consumer-rules.pro` inside each plugin `android/` dir referenced by `gen/android/tauri.settings.gradle`; currently covers `tauri-plugin-dialog` and `tauri-plugin-fs`. It runs in `prepare()` before every Android build.
- `src-tauri/build.rs::stub_windows_bundle_resources` touches the Windows bundle placeholder needed by `tauri_build` resource validation during `just check` / dev builds on a fresh checkout. `install_cuda_dlls` copies real CUDA DLLs from `%APPDATA%` during release builds. Pre-bundle, verify file sizes before shipping a release.
- `src-tauri/build.rs` warns when `CMAKE_GENERATOR` changes; `xtask/src/check.rs` owns the cache wipe for `target/{debug,release}/build/{whisper-rs-sys-*,sherpa-onnx-sys-*}` using the `target/.cmake-generator` sentinel.
- `xtask/src/release/builders.rs::ensure_dev_keystore_properties` regenerates `src-tauri/gen/android/keystore.properties` whenever the recorded `storeFile` is missing on the current host. It is called from both `cargo xtask android build` and the release matrix so the same checkout signs APKs on Windows and Linux.

## Guardrail candidates

Promote these to mechanical checks when they become recurring review feedback:

- Command mirror check: detect Tauri commands or IPC structs changed without corresponding `src/api.ts` / `src/types.ts` / `src/schemas.ts` updates.
- Documentation freshness metadata: add owner/last-verified fields if docs become stale despite `scripts/lint-docs.ts` link/catalogue checks.
- UI smoke probes: automate critical flows through CDP/Playwright for transcribe/settings/logs.
- Cache-key regression fixtures for engine/timestamp/diarization settings.

## Windows host setup

`scripts/bootstrap-windows.ps1` (run by `just bootstrap`, which is a dependency of `just build`) installs or repairs: VS 2022 Build Tools, rustup (msvc), Bun, Node, NSIS, CMake, Ninja, LLVM/libclang, MSYS2, just, CUDA Toolkit 12.x via `Nvidia.CUDA`, cuDNN 9 via `scripts/install-cudnn.ps1`, and sherpa-onnx CUDA runtime via `scripts/install-sherpa-cuda.ps1`.

Subsequent runs are idempotent. `bun scripts/doctor.ts` validates the same prerequisites are reachable from the current shell.
