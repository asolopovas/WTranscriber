# Rust build speed

## Rules

- Do not re-enable LTO in `[profile.release]`. Heavy work is C++; LTO costs minutes for sub-1% gain.
- Do not cap `CARGO_BUILD_JOBS`.
- Inner loop: `cargo check` / `cargo clippy`, not `cargo build`. `just check` runs both in parallel with the rest of the gate.
- Profile with `cargo build --timings`; re-measure after each change.

## Toolchain wrappers (committed, installed by `just bootstrap`)

- **sccache** wraps `rustc` (`RUSTC_WRAPPER`) and cmake-driven C/C++ via
  `CMAKE_{C,CXX}_COMPILER_LAUNCHER`. Survives `cargo clean` and shares
  artefacts between host + Android targets where deps overlap. Biggest single
  win on warm rebuilds and after toolchain bumps. `sccache --show-stats`
  shows hit rate. sccache refuses to cache incremental rustc output
  ([upstream](https://github.com/mozilla/sccache#known-caveats)), so Android
  dev/build sets `CARGO_INCREMENTAL=0` in `xtask::android::build::build_env`
  to keep the Rust hit rate high on cross-target rebuilds; desktop `just dev`
  keeps incremental on (the per-target debug tree is warm there).
- **LLVM `lld-link`** is selected via
  `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=lld-link.exe`, set as a User env
  var by the bootstrap script once LLVM is on PATH. Materially faster than
  `link.exe` on warm rebuilds. (Env-based rather than committed to
  `.cargo/config.toml` so fresh checkouts still build with `link.exe` before
  bootstrap runs.)

To disable either: clear the relevant User env var
(`[Environment]::SetEnvironmentVariable('NAME', $null, 'User')`).

## Dev profile (committed in `src-tauri/Cargo.toml`)

```toml
[profile.dev]
incremental = true
debug = "line-tables-only"
split-debuginfo = "unpacked"
codegen-units = 256

[profile.dev.package."*"]
opt-level = 3
```

Optimises deps once; hot-path crates (`sherpa-onnx`, `tokio`, `reqwest`, `rubato`, `vad-rs`) stay fast. If `tauri dev` reload misbehaves, drop dep `opt-level` to `1`.

## Reference times

Warm rebuild after one Rust source change (Windows, 16 cores):

| Command                                           | Time  | Output                              |
| ------------------------------------------------- | ----- | ----------------------------------- |
| `cargo xtask release --dev --no-android --no-deb` | ~50 s | Host only: GUI installer + `wt` CLI |
| `just build`                                      | ~5 m  | Windows-only full dev matrix        |

Cold builds are much slower because whisper.cpp, ggml, and sherpa-onnx all configure and compile native code.

## Dependencies

- `cargo tree --duplicate` after `cargo update` — flag duplicate versions.
- `tokio = { features = ["full"] }` is the heaviest feature flag.

## Cache hygiene

`src-tauri/target` reaches 15–20 GB in normal use (debug ~6 GB, release ~11 GB).

- Full nuke (cold next build, ~210 s for release): `cargo clean --manifest-path src-tauri/Cargo.toml && cargo clean --manifest-path xtask/Cargo.toml && rm -rf tmp dist node_modules`.
- Surgical reclaim without wiping deps: `cargo clean --release --manifest-path src-tauri/Cargo.toml` reclaims ~11 GB; one cold release build to refill.
- `~/.cargo` registry/git caches can be trimmed with `cargo cache -a` (needs `cargo-cache`).

`target/sherpa-onnx-prebuilt/` (~900 MB) is a download cache for the prebuilt sherpa-onnx binaries, **not** managed by cargo. Deleting it forces a re-download on the next build.
