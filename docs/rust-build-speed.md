# Rust build speed

## Rules

- Do not re-enable LTO in `[profile.release]`. Heavy work is C++; LTO costs minutes for sub-1% gain.
- Do not cap `CARGO_BUILD_JOBS`.
- Inner loop: `cargo check` / `cargo clippy`, not `cargo build`. `just lint` does this.
- Profile with `cargo build --timings`; re-measure after each change.

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

| Recipe            | Time  | Output                                                         |
| ----------------- | ----- | -------------------------------------------------------------- |
| `just build-host` | ~50 s | `wt` CLI + GUI installer (NSIS .exe on Windows, .deb on Linux) |
| `just build`      | ~5 m  | Full matrix: host (GUI + CLI) + Android APK + Linux .deb       |

Cold build: ~210 s. Floor is the single-threaded link of statically-bundled `sherpa-onnx`.

## Dependencies

- `cargo tree --duplicate` after `cargo update` — flag duplicate versions.
- `tokio = { features = ["full"] }` is the heaviest feature flag.

## Cache hygiene

`src-tauri/target` reaches 15–20 GB in normal use (debug ~6 GB, release ~11 GB).

- `just clean` — full nuke: `tmp/`, `src-tauri/target`, `xtask/target`, `dist`, `node_modules`. Next build is fully cold (~210 s for release).
- For surgical reclaim without wiping deps: `cargo clean --release --manifest-path src-tauri/Cargo.toml` reclaims ~11 GB; one cold release build to refill.
- `~/.cargo` registry/git caches can be trimmed with `cargo cache -a` (needs `cargo-cache`).

`target/sherpa-onnx-prebuilt/` (~900 MB) is a download cache for the prebuilt sherpa-onnx binaries, **not** managed by cargo. Deleting it forces a re-download on the next build.
