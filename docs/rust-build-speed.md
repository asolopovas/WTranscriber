# Native build and cache constraints

- Keep release `lto = false` and do not cap `CARGO_BUILD_JOBS`. Profiles are owned by `src-tauri/Cargo.toml`; avoid duplicating them in documentation.
- Linux x86_64 C++ needs `_GLIBCXX_USE_CXX11_ABI=0`, matching prebuilt Sherpa/ONNX Runtime. `.cargo/config.toml` supplies target-specific CXXFLAGS. After changing that flag, clean `whisper-rs-sys` explicitly: its build script does not track the environment change. Mixing ABIs can abort in `std::regex` during model initialisation.
- `just setup` prewarms whisper/sherpa C++ dependencies. After wiping targets, prewarm again before parallel `just check` jobs contend for Cargo locks.

## Windows wrappers

`scripts/bootstrap-windows.ps1` sets user environment variables:

| Tool         | Configuration                                                               |
| ------------ | --------------------------------------------------------------------------- |
| sccache      | `RUSTC_WRAPPER`, `CMAKE_C_COMPILER_LAUNCHER`, `CMAKE_CXX_COMPILER_LAUNCHER` |
| lld-link.exe | `CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER` once LLVM is available         |

The linker stays environment-based so unbootstrapped clones can use link.exe. Android sets `CARGO_INCREMENTAL=0` in xtask's build environment for sccache; desktop development keeps incremental compilation. Disable a wrapper by removing its user environment variable.

## Cache ownership

Generator changes are managed by `xtask/src/check.rs` using `target/.cmake-generator`; see [technical debt](technical-debt.md). `target/sherpa-onnx-prebuilt/` is a download cache outside Cargo's cleanup ownership; deleting it forces download.

Use `cargo build --timings` and `sccache --show-stats` for actual build/cache measurements.
