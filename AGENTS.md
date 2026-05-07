# AGENTS.md

Notes for agents working on this repo.

## Stack

- Tauri 2 (Rust, edition 2024, MSRV 1.85)
- Vue 3 + TypeScript + Vite
- Bun (JS runtime + package manager)
- `just` (task runner)

## Layout

```
src/                Vue 3 frontend
  api.ts            wrappers around Tauri commands
  types.ts          TS types matching Rust structs
  App.vue           root component
src-tauri/
  src/
    main.rs         Tauri desktop binary
    bin/wt.rs       headless CLI (clap)
    lib.rs          tauri::Builder, plugins, command registration
    api.rs          re-exports for the CLI / external code
    commands.rs     #[tauri::command] handlers (thin)
    config.rs       persisted user config
    models.rs       model registry / discovery
    paths.rs        config / data paths (LazyLock)
    error.rs        thiserror Error, serializable for IPC
    transcriber/    transcription pipeline
  capabilities/     Tauri permissions
  tauri.conf.json
  rustfmt.toml
xtask/              build / release orchestration (Rust binary)
  src/
    main.rs         clap dispatcher
    util.rs         shared helpers (paths, git, streamed processes)
    bump.rs         version sync across package.json / Cargo.toml / tauri.conf.json
    release.rs      parallel host + Android + WSL builds, SHA256SUMS, manifest
    publish.rs      gh CLI wrapper for dev / stable releases
    android.rs      build / dev / cli / prebuilts / sign-patch
.cargo/config.toml  cargo alias: `cargo xtask` -> xtask binary
justfile            thin user-facing wrappers around `cargo xtask`
scripts/
  cdp.mjs           Chrome DevTools Protocol eval (debug only, needs Node)
  diarize.py        speaker diarization sidecar (runtime resource)
  install-*.ps1     Windows-only runtime deps (CUDA / cuDNN / NeMo)
docs/release.md     release process + build-speed reference
```

## Rules

- **No comments in code.** Names carry intent.
- **No `sleep` in scripts.** Wait on a real signal: process exit, file appears,
  log line, or poll with timeout. Applies to bash, `.mjs`, `.ps1`, `adb shell`.
- **Use edition 2024** features (`LazyLock`, `let-else`, etc.).
- **Errors crossing Rust → JS** go through `error::Error` (impl `Serialize`).
- **Frontend types** in `src/types.ts` must match the Rust structs.
- **Lints**: `cargo clippy -- -D warnings`, pedantic + nursery on.
- **Formatters**: `cargo fmt`, `prettier` (TS/Vue/MD/JSON/HTML).

## Build performance

- Cargo runs on all cores by default. Don't cap `-j` or set `CARGO_BUILD_JOBS=1`.
- A long single-line stall during link is normal — sherpa-onnx is statically linked.
  Verify progress with `cargo build -v` (it logs `Compiling X v…` per crate).
- Cold release build ≈ 3.5 min on Windows (16 cores). Warm rebuild after a Rust
  change: 6–28 s depending on bundle target. See `docs/release.md` for the table.
- The `[profile.release]` is tuned for build speed (incremental on, LTO off).
  Don't re-enable LTO without proving it matters at runtime — heavy lifting is
  C++ (sherpa-onnx, webkit), so Rust LTO is sub-1 % runtime gain at minutes per
  build.

## Recipes

```
just                list recipes
just setup          install JS deps + git hooks

# develop
just dev            run app (Vue HMR + tauri dev)
just watch          cargo watch, rebuild on save
just build-bin      raw cargo build, no Tauri post-process    (~6 s warm)
just build-app      Tauri-patched exe, no installer           (~9 s warm)
just build          NSIS installer                            (~28 s warm)
just build-all      NSIS + MSI (legacy / enterprise)
just build-cpu      build with sherpa-static (no CUDA)
just build-cuda     build with --features cuda
just build-cli      build the headless `wt` CLI

# quality
just fmt / fmt-check
just lint           clippy + vue-tsc
just test           cargo test --offline
just check          fast gate: fmt-check + lint + test         (pre-commit)
just check-all      check + cargo-machete + cargo-audit + bun audit
just dep-check      unused crate deps
just audit          vulnerability scan

# release (all delegate to `cargo xtask`)
just release            dev build → rolling 'dev' prerelease
just release-stable     check + bump + tag + build + publish stable
just release-bump       bump + commit + tag only
just release-build      build artifacts only (--dev flag for dev channel)
just release-publish    upload existing artifacts to dev or vX.Y.Z
                        direct: cargo xtask {release|bump|publish|android} --help
                        full reference: docs/release.md

# misc
just clean          remove target + dist + node_modules
just icons          regenerate icons from src-tauri/icons/icon.png
just android-*      Android scaffold / build / debug
```

## Quality gates

### `just check` (fast, no network) — runs pre-commit

1. `cargo fmt --check` + `prettier --check`
2. `cargo clippy --all-targets --offline -- -D warnings`
3. `bun run typecheck` (`vue-tsc`)
4. `cargo test --offline`

Warm cache: a few seconds.

### `just check-all` (manual, pre-release)

Adds:

5. `cargo machete` — unused crate deps in `Cargo.toml`
6. `cargo audit` + `bun audit` — vulnerability scan

Missing tools auto-install on first run.

## Git hooks (`.githooks/`)

Only relevant work runs.

**`pre-commit`** looks at staged paths:

- Rust / `Cargo.toml` / `Cargo.lock` → `cargo fmt --check` + clippy
- TS / Vue → `prettier --check` (changed files) + `vue-tsc`
- Markdown / JSON / HTML → `prettier --check` (changed files)
- Nothing relevant → skip

**`pre-push`** runs `cargo test --offline` once. Test compilation isn't on the
commit path so iteration stays fast.

`just setup` (or `just install-hooks`) sets `core.hooksPath = .githooks`.
Bypass with `--no-verify` only in emergencies.

## Adding a Tauri command

1. Write the function in `src-tauri/src/commands.rs` (or a domain module
   that re-exports it).
2. Register it in `lib.rs` `invoke_handler![…]`.
3. Add a typed wrapper in `src/api.ts`.
4. If it returns a domain type, add the type to `src/types.ts`.

## Android debugging

Full guide: `docs/tauri-debug.md`. Quick reference:

- `just android-debug-attach` — finds `webview_devtools_remote_<pid>`,
  runs `adb forward tcp:9222 …`, prints the page list.
  Open `chrome://inspect` to attach.
- `node scripts/cdp.mjs "<expr>"` — evaluates JS in the live WebView via CDP.
  Use it to read Vue state or dispatch DOM events.
- Logcat tags: `chromium` / `Console` for JS, `RustStdoutStderr` for Rust
  `println!` + `tauri-plugin-log` Stdout target.
- Screenshot: `MSYS_NO_PATHCONV=1 adb exec-out screencap -p > tmp/x.png`.
  `*.png` at the repo root is gitignored — keep captures under `tmp/`.

## Migration from `wt` (Go)

The Rust layout mirrors the Go one to make porting mechanical:

| Go (`internal/`) | Rust (`src-tauri/src/`)             |
| ---------------- | ----------------------------------- |
| `config.go`      | `config.rs`                         |
| `models/`        | `models.rs`                         |
| `transcriber/`   | `transcriber/`                      |
| `diarizer/`      | `transcriber/diarizer.rs` (planned) |
| `gui/`           | `src/` (Vue)                        |
| `appinfo/`       | `lib.rs` (`app_version` command)    |

Engine binaries (`sherpa-onnx-offline`, `llama-cli`, NeMo Sortformer) run as
sidecars via `tauri-plugin-shell`. They are not bundled here; the user fetches
them post-install (same pattern as the Windows installer in `wt`).
