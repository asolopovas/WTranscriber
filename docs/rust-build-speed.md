# Rust build speed

## Rules

- Do not re-enable LTO in `[profile.release]`. Heavy work is C++; LTO costs minutes for sub-1% gain.
- Do not cap `CARGO_BUILD_JOBS`.
- Inner loop: `cargo check` / `cargo clippy`, not `cargo build`. `just lint` does this.
- Profile with `cargo build --timings`; re-measure after each change.

## Linker (committed in `.cargo/config.toml`)

| Target                     | Linker           | Setup                    |
| -------------------------- | ---------------- | ------------------------ |
| `x86_64-pc-windows-msvc`   | `rust-lld`       | none                     |
| `x86_64-unknown-linux-gnu` | `clang` + `mold` | `apt install clang mold` |

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

## Reference times

Warm rebuild after one Rust source change (Windows, 16 cores):

| Recipe           | Time | Output                      |
| ---------------- | ---- | --------------------------- |
| `just build-bin` | 6 s  | raw binary                  |
| `just build-app` | 9 s  | Tauri-patched, no installer |
| `just build`     | 28 s | NSIS installer              |
| `just build-all` | 45 s | NSIS + MSI                  |

Cold build: ~210 s. Floor is the single-threaded link of statically-bundled `sherpa-onnx`.

## Dependencies

- `cargo tree --duplicate` after `cargo update` — flag duplicate versions.
- `tokio = { features = ["full"] }` is the heaviest feature flag.
