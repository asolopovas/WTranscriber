# Rust Build Speed

Module of [`AGENTS.md`](../AGENTS.md). Compile-time tuning for the stack defined there. Release artifact times live in [`release.md`](release.md).

## Constraints

- Do not re-enable LTO in `[profile.release]`. Heavy work is C++; LTO costs minutes for sub-1% gain.
- Do not cap `CARGO_BUILD_JOBS`.
- Inner loop: `cargo check` or `cargo clippy`, not `cargo build`. `just lint` does this. Fastest single check: `cargo check -p wtranscriber`.
- Use rust-analyzer for sub-second editor feedback.
- Profile with `cargo build --timings` (HTML Gantt of per-crate compile/link times). Re-measure after each change.

## Linker (committed in `.cargo/config.toml`)

| Target                     | Linker               | Setup                          |
| -------------------------- | -------------------- | ------------------------------ |
| `x86_64-pc-windows-msvc`   | `rust-lld` (bundled) | none (toolchain >= 1.79)       |
| `x86_64-unknown-linux-gnu` | `clang` + `mold`     | `apt install clang mold`       |
| `aarch64-apple-darwin`     | system `ld` + `lld`  | `brew install llvm` if missing |

## Dev profile (committed in `src-tauri/Cargo.toml`)

```toml
[profile.dev]
incremental = true
debug = "line-tables-only"
split-debuginfo = "unpacked"

[profile.dev.package."*"]
opt-level = 3
```

Optimises deps once; hot-path crates (`sherpa-onnx`, `tokio`, `reqwest`, `rubato`, `vad-rs`) stay fast. If `tauri dev` reload misbehaves, drop dep `opt-level` to `1`.

## Dependency graph

- `cargo tree --duplicate`: flag duplicate versions after `cargo update`.
- `cargo machete` (via `just dep-check`): run before each release.
- `tokio = { features = ["full"] }` is the heaviest feature flag. Narrow once async surface stabilises.
- Audit new dep default features on docs.rs before adding.

## Cranelift (optional, nightly)

20-30% faster debug codegen. Mature on Linux, less on Windows MSVC.

```toml
# .cargo/config.toml
[unstable]
codegen-backend = true

# src-tauri/Cargo.toml
[profile.dev]
codegen-backend = "cranelift"
```

Install: `rustup component add rustc-codegen-cranelift-preview --toolchain nightly`. Not enabled by default (repo targets stable).

## Workspace split (deferred)

If compile times become painful, split `src-tauri/src/`:

- `wtranscriber-core`: `transcriber/`, `models.rs`, `paths.rs` (no Tauri).
- `wtranscriber-tauri`: `commands.rs`, `lib.rs`.
- `wt` CLI: depends on core only.

Then `cargo check -p wtranscriber-core` and `cargo run --bin wtr` skip the Tauri graph.

Diagnose generic bloat with `cargo llvm-lines` if a module shows hot in `--timings`.

## CI

- `just check`: fast gate (fmt + clippy + typecheck + test, offline).
- `just check-all`: adds machete + audit for pre-release.

For GitHub Actions:

- `Swatinem/rust-cache@v2` keyed on `Cargo.lock` + `rust-toolchain`.
- `CARGO_INCREMENTAL=0` (incremental hurts clean cached builds, bloats cache).
- `RUSTFLAGS=-D warnings` via env, not source.

## Hardware

RAM-, core-, then I/O-bound. NVMe + 32 GB + many cores is the sweet spot. On Linux, `target/` on `tmpfs` is free speed if RAM allows.
