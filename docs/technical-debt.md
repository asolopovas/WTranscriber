# Technical debt

Temporary patches, cleanup work, and removal triggers. Each item must be specific enough to retire without external memory.

## Platform build workarounds

Review these against upstream changes and verify the affected platform before removal. Tauri 2.12 plugin packages now include their consumer ProGuard rules, so the old rule-file workaround has been removed.

- `src-tauri/gen/android/app/src/main/java/com/asolopovas/wtranscriber/generated/WryActivity.kt` carries inline `@Suppress("DEPRECATION")` annotations on the `packageManager.getPackageInfo(...)` calls in the WebView-version getter so they do not fail `-Werror` Kotlin builds.
- `src-tauri/build.rs::stub_windows_bundle_resources` touches the Windows bundle placeholder needed by `tauri_build` resource validation during `just check` / dev builds on a fresh checkout. `install_cuda_dlls` copies real CUDA DLLs from `%APPDATA%` during release builds. Pre-bundle, verify file sizes before shipping a release.
- `src-tauri/build.rs` warns when `CMAKE_GENERATOR` changes; `xtask/src/check.rs` owns the cache wipe for `target/{debug,release}/build/{whisper-rs-sys-*,sherpa-onnx-sys-*}` using the `target/.cmake-generator` sentinel.
- `xtask/src/release/builders.rs::ensure_dev_keystore_properties` regenerates `src-tauri/gen/android/keystore.properties` whenever the recorded `storeFile` is missing on the current host. It is called from both `cargo xtask android build` and the release matrix so the same checkout signs APKs on Windows and Linux.

## Dependency compatibility constraints

The 2026-10-03 dependency refresh uses Rust 1.99 with an MSRV of 1.90 for Tauri 2.12. Current stable package versions are resolved in the Cargo lockfiles; the Bun lockfile remains locally generated under the repository's ignore policy.

- Keep TypeScript at `~6.0.3` while `vue-tsc` 3.3.12 requires the JavaScript compiler entry point `typescript/lib/tsc`. TypeScript 7.0.2 removes that export and fails before checking the application. Upgrade once Vue's supported checker handles the new compiler, then run typecheck and the frontend build.
- Keep `parakeet-rs =0.3.7` while the model catalogue supplies four-speaker Sortformer v2 models. Version 0.3.8 requires eight-speaker models and rejects existing installations. Upgrade only alongside an explicit model migration and diarisation regression coverage.
- Keep ONNX Runtime's `api-24` feature explicit, with `ort` defaults disabled. The DirectML installer uses 1.24.4 and the Windows import library remains ABI-compatible 1.24.2; enabling `parakeet-rs/ort-defaults` would raise the native API requirement. Raise the API only when all packaged CPU, CUDA, DirectML, and Android runtimes support it.
- `paste` 1.0.15 remains an upstream unmaintained dependency of `tokenizers` through `parakeet-rs`; the existing audit policy allows that advisory. Remove the exception when upstream replaces it.

- Keep `sherpa-onnx =1.13.3`, the explicit `sherpa-onnx-sys =1.13.3` native ABI constraint, and `sherpa-version.txt` aligned while Windows shares one ONNX Runtime between Sherpa and DirectML. Sherpa 1.13.4+ packages require ONNX Runtime 1.27 or newer; the latest DirectML package is 1.24.4. A later Sherpa upgrade needs compatible DirectML binaries or separate runtime processes, followed by CPU, CUDA, and DirectML smoke tests.

- Keep the direct `ort-sys` dependency's `disable-linking` feature. Sherpa owns the linked ONNX Runtime library; allowing both build scripts to link separate static copies causes duplicate symbols and can mix incompatible implementations. This leaves `ort`'s ordinary C API binding intact and preserves explicit dynamic loading on Windows DirectML and Android.
