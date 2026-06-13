# Rust build speed

## Rules

- Do not re-enable LTO in `[profile.release]` (`lto = false`). Heavy work is C++; LTO costs minutes for sub-1% gain.
- Do not cap `CARGO_BUILD_JOBS`.
- Inner loop: `cargo check` / `cargo clippy`, not `cargo build`. `just check` runs both in parallel.
- Profile with `cargo build --timings`; re-measure after each change.

## Toolchain wrappers (installed by `scripts/bootstrap-windows.ps1`, set as User env vars)

- **sccache** — `RUSTC_WRAPPER=sccache` plus `CMAKE_{C,CXX}_COMPILER_LAUNCHER=sccache` for cmake C/C++. Survives `cargo clean`; shares artefacts across host + Android where deps overlap. `sccache --show-stats` for hit rate. sccache will not cache incremental rustc output, so Android build sets `CARGO_INCREMENTAL=0` in `xtask::android::build::build_env`; desktop `just dev` keeps incremental on.
- **`lld-link.exe`** — `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER=lld-link.exe`, set once LLVM is on PATH. Faster than `link.exe` on warm rebuilds. Env-based (not in `.cargo/config.toml`) so fresh checkouts build with `link.exe` before bootstrap runs.

Disable either: `[Environment]::SetEnvironmentVariable('NAME', $null, 'User')`.

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

Optimises deps once; hot-path crates stay fast. If `tauri dev` reload misbehaves, drop dep `opt-level` to `1`.

## Reference times

Warm rebuild after one Rust source change (Windows, 16 cores):

| Command                                           | Time  | Output                              |
| ------------------------------------------------- | ----- | ----------------------------------- |
| `cargo xtask release --dev --no-android --no-deb` | ~50 s | Host only: GUI installer + `wt` CLI |
| `just build`                                      | ~5 m  | Windows-only full dev matrix        |

Cold builds compile whisper.cpp, ggml, and sherpa-onnx native code — much slower.

## Dependencies

- `cargo tree --duplicate` after `cargo update` — flag duplicate versions.
- `tokio = { features = ["full"] }` is the heaviest feature flag.

## Cache hygiene

`src-tauri/target` reaches 15–20 GB (debug ~6 GB, release ~11 GB).

- Full nuke (~210 s to refill release): `cargo clean --manifest-path src-tauri/Cargo.toml && cargo clean --manifest-path xtask/Cargo.toml && rm -rf tmp dist node_modules`.
- Surgical: `cargo clean --release --manifest-path src-tauri/Cargo.toml` reclaims ~11 GB.
- `~/.cargo` registry/git caches: `cargo cache -a` (needs `cargo-cache`).

`target/sherpa-onnx-prebuilt/` (~900 MB) is a download cache for prebuilt sherpa-onnx binaries, **not** managed by cargo. Deleting it forces re-download on the next build.
