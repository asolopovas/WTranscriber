# Rust Build Speed — WTranscriber

Tuning notes for fast iteration on this repo (Tauri 2 + Rust 2024, MSRV 1.85,
Windows primary, Linux/macOS supported). Apply in order; stop when builds feel
snappy.

## 1. Free wins

- **Update toolchain.** `rustup update`. Recent stables ship LLD by default on
  `x86_64-unknown-linux-gnu` and include `rust-lld` on Windows.
- **`cargo check` / `cargo clippy` in the inner loop**, not `cargo build`.
  `just lint` already does this; `cargo check -p wtranscriber` is the fastest
  "did I break it" check during edits.
- **Rely on rust-analyzer** in the editor for sub-second feedback without
  invoking cargo.
- **Profile first.** `cargo build --timings` produces an HTML Gantt chart of
  per-crate compile/link times. Re-measure after each change below.

## 2. Faster linker (biggest single win)

Linking dominates incremental rebuilds, especially with the Tauri dependency
tree.

Configured in `.cargo/config.toml` at the repo root:

| Target                     | Linker               | Setup                              |
| -------------------------- | -------------------- | ---------------------------------- |
| `x86_64-pc-windows-msvc`   | `rust-lld` (bundled) | none (ships with toolchain ≥ 1.79) |
| `x86_64-unknown-linux-gnu` | `clang` + `mold`     | `apt install clang mold` (or pkg)  |
| `aarch64-apple-darwin`     | system `ld` + `lld`  | `brew install llvm` if missing     |

Mold is Linux-only and the fastest option there. Windows uses `rust-lld`
because installing external `lld` adds friction; `rust-lld` is "good enough"
and zero-config.

## 3. Optimise dependencies, not your code (dev profile)

Set in `src-tauri/Cargo.toml`:

```toml
[profile.dev]
incremental = true
debug = "line-tables-only"     # enough for backtraces, smaller target/, faster link
split-debuginfo = "unpacked"   # macOS/Linux; ignored on Windows MSVC

[profile.dev.package."*"]
opt-level = 3                  # optimise deps once
```

Big wins for this repo because `sherpa-onnx`, `tokio`, `reqwest`, `rubato`,
and `vad-rs` are all in the hot path of the transcription pipeline.

> **Tauri caveat:** if webview debugging or `tauri dev` reload feels weird
> after this change, drop dep `opt-level` to `1` and re-measure.

## 4. Slim the dependency graph

Already in good shape (most deps disable default features), but keep an eye
on:

- `cargo tree --duplicate` — flag duplicate versions after `cargo update`.
- `cargo machete` — `just dep-check` runs this. Run before each release.
- `tokio = { features = ["full"] }` is the heaviest single feature flag in
  this repo. If/when the async surface stabilises, narrow it to the runtime,
  macros, fs, process, and signal features actually used.
- Audit any new dep's default features on docs.rs before adding it.

## 5. Cranelift codegen (optional, nightly)

Roughly 20–30% faster codegen for debug builds. Mature on Linux, less so on
Windows MSVC — try it, measure it, revert if it misbehaves.

```toml
# .cargo/config.toml
[unstable]
codegen-backend = true

# Cargo.toml (src-tauri/)
[profile.dev]
codegen-backend = "cranelift"
```

Install: `rustup component add rustc-codegen-cranelift-preview --toolchain nightly`.

Not enabled by default in this repo because we target stable.

## 6. Workspace structure

The Rust crate is currently a single binary + lib. If compile times become
painful, split along the seams already present in `src-tauri/src/`:

- `wtranscriber-core` — `transcriber/`, `models.rs`, `paths.rs` (no Tauri).
- `wtranscriber-tauri` — `commands.rs`, `lib.rs` (depends on core).
- `wt` CLI — depends on core only, skips the Tauri graph entirely.

This would make `cargo check -p wtranscriber-core` and `cargo run --bin wt`
dramatically faster than today, since neither pulls Tauri.

Diagnose generic bloat with `cargo llvm-lines` if any module shows up hot in
`--timings`.

## 7. CI

The justfile gates already align with fast-CI practice:

- `just check` is the fast gate (fmt + clippy + typecheck + test, offline).
- `just check-all` adds machete + audit for pre-release.

For GitHub Actions, add:

- `Swatinem/rust-cache@v2` keyed on `Cargo.lock` + `rust-toolchain`.
- `CARGO_INCREMENTAL=0` in the CI env (incremental hurts clean cached builds
  and bloats the cache).
- `RUSTFLAGS=-D warnings` via env, not in source.

## 8. Hardware

Rust compilation is RAM-, core-, then I/O-bound. NVMe + 32 GB + many cores
is the sweet spot. On Linux, mounting `target/` on `tmpfs` is free speed if
RAM allows.

## TL;DR for this repo

1. Use `cargo check` / `just lint` while editing.
2. `.cargo/config.toml` selects a fast linker per platform (committed).
3. `[profile.dev.package."*"] opt-level = 3` makes dep-heavy hot paths fast
   without hurting your own rebuild times (committed).
4. Run `cargo build --timings` once a quarter to spot regressions.

Everything else (Cranelift, workspace split) is opt-in once a specific pain
point shows up in `--timings`.
