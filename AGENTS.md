# AGENTS.md

Operational notes for agents working on this repo.

## Stack

- Tauri 2 (Rust, edition 2024, MSRV 1.85)
- Vue 3 + TypeScript + Vite
- Bun (JS package manager / runner)
- `just` (task runner)

## Layout

```
src/                Vue 3 frontend
  api.ts            invoke wrappers for Tauri commands
  types.ts          shared types mirroring Rust serde structs
  App.vue           root component
src-tauri/
  src/
    main.rs         desktop binary entry (Tauri)
    bin/wt.rs       headless CLI binary (clap)
    lib.rs          tauri::Builder, plugin registration, command list
    api.rs          public re-exports for the CLI / external consumers
    commands.rs     #[tauri::command] handlers (thin)
    config.rs       persisted user config
    models.rs       model registry / discovery
    paths.rs        cross-platform config / data paths (LazyLock)
    error.rs        thiserror Error + serde Serialize for IPC
    transcriber/    transcription pipeline (port of internal/transcriber)
  capabilities/     Tauri permissions
  tauri.conf.json
  rustfmt.toml
justfile            task recipes
```

## Conventions

- **No comments in code.** Names carry intent.
- Edition 2024 features encouraged (`std::sync::LazyLock`, `let-else`, etc.).
- All Rust→JS errors go through `error::Error` (impl `Serialize`).
- Frontend types in `src/types.ts` must mirror Rust structs (kebab/snake mapped via serde).
- Lints: `cargo clippy -- -D warnings` with pedantic + nursery enabled.
- Format: `cargo fmt`, `prettier` for TS/Vue.

## Tasks

```
just              list recipes
just setup        install JS deps + git pre-commit hook
just dev          run app
just build        production bundle (CPU / static sherpa-onnx)
just build-cuda   production bundle with --features cuda
just fmt          format Rust + TS/Vue
just fmt-check    cargo fmt --check + prettier --check
just lint         clippy (warnings as errors) + vue-tsc
just test         cargo test (offline)
just dep-check    cargo-machete (unused deps; manual)
just audit        cargo-audit + bun audit (vulns; manual)
just check        fast gate: fmt-check + lint + test (see below)
just check-all    full gate: check + dep-check + audit (pre-release)
just clean        remove target + dist + node_modules
just icons        regenerate icon set from src-tauri/icons/icon.png
just android-*    Android scaffold / build (see docs/android.md)
```

## Quality gate — tiered

### `just check` (fast, pre-commit, no network)

1. `cargo fmt --check` + `prettier --check`.
2. `cargo clippy --all-targets --offline -- -D warnings` (pedantic + nursery,
   `dead_code` / `unused_imports` enforced).
3. `bun run typecheck` (`vue-tsc`).
4. `cargo test --offline`.

Leans on cargo's incremental cache; warm runs are seconds.

### `just check-all` (slower, manual / pre-release)

Adds: 5. `cargo machete` — unused crate deps in `Cargo.toml`. 6. `cargo audit` (RustSec DB) + `bun audit` — vulnerability scan.

Missing tools (`cargo-machete`, `cargo-audit`) auto-install on first run.

### Git hooks (`.githooks/`)

Incremental — only run what's relevant.

**`pre-commit`** inspects `git diff --cached --name-only`:

- Rust file or `Cargo.toml` / `Cargo.lock` staged → `cargo fmt --check` +
  `cargo clippy --offline -D warnings`. Warm cache: ~1–2s.
- TS/Vue staged → `prettier --check` (changed files only) + `vue-tsc`.
- Markdown / JSON / HTML staged → `prettier --check` (changed files).
- Nothing relevant → skip.

**`pre-push`** runs `cargo test --offline` once before publishing.
Faster feedback loop than blocking every commit on test compilation.

`just setup` (and `just install-hooks`) point `core.hooksPath` at
`.githooks`. Bypass with `git commit --no-verify` /
`git push --no-verify` only for emergencies.

## Adding a Tauri command

1. Implement function in `src-tauri/src/commands.rs` (or domain module re-exported).
2. Register it in `lib.rs` `invoke_handler![…]`.
3. Add a typed wrapper in `src/api.ts`.
4. If it returns a domain type, add the type to `src/types.ts`.

## Migration from `wt` (Go)

The Rust skeleton mirrors the Go module layout to make porting mechanical:

| Go (`internal/`) | Rust (`src-tauri/src/`)             |
| ---------------- | ----------------------------------- |
| `config.go`      | `config.rs`                         |
| `models/`        | `models.rs`                         |
| `transcriber/`   | `transcriber/`                      |
| `diarizer/`      | `transcriber/diarizer.rs` (planned) |
| `gui/`           | `src/` (Vue)                        |
| `appinfo/`       | `lib.rs` (`app_version` cmd)        |

Engine binaries (`sherpa-onnx-offline`, `llama-cli`, NeMo Sortformer) are
invoked as sidecars via `tauri-plugin-shell`. They are not bundled in this
repo; they are downloaded post-install (same pattern as the Windows installer
in `wt`).
