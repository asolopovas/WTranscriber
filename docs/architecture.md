# Architecture

WTranscriber is a Tauri app: Vue/WebView owns presentation; Rust owns filesystem, models, native runtimes, audio processing, and long-running transcription work.

## Layout

```text
src/             Vue 3 frontend; api.ts and types.ts mirror Rust IPC
src/components/  Vue UI components
src/composables/ Vue state/effect helpers
src/utils/       Frontend utilities and tests
src-tauri/src/   Rust app code
  commands/      Tauri command handlers grouped by domain
  models/        Model metadata, download, storage helpers
  transcriber/   Transcript cache, jobs, chunk/slab orchestration
  diarizer/      Speaker diarization
  audio/         Audio decoding and manipulation
  audio_toolkit/ Native audio tooling integration
  runtimes/      Runtime discovery and installation
  llm/           LLM integration
  engine/        ASR engine adapters
  namer/         Naming/title helpers
xtask/src/       check, bump, publish, release, Android orchestration
scripts/         Bun/TS developer scripts and Windows bootstrap helpers
docs/            Agent-operable project knowledge
.agents/skills/  Project-local pi skills; mirrored to .opencode/skills
.vscode/         Task wrappers for dev, check, Android install
```

Key Rust entry points: `src-tauri/src/lib.rs`, `src-tauri/src/bin/wt.rs`, `api.rs`, `config.rs`, `paths.rs`, `error.rs`, `constants.rs`, `android.rs`, `browser.rs`, `essentials.rs`, `fs_utils.rs`, `lang_id.rs`, `logfile.rs`, `process.rs`, `progress.rs`, `runtime_install.rs`.

## Boundary rules

- Frontend talks to Rust through typed Tauri commands and events only.
- Errors crossing JS use `error::Error` and must be serializable.
- `src/types.ts` mirrors Rust structs. Keep shape changes synchronized with `src/api.ts` wrappers and command return types.
- Use frontend aliases `@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`.
- Capability permissions are least-privilege. If IPC fails, inspect console plus `RustStdoutStderr` before widening permissions.
- Large binary payloads cross IPC as raw bodies (`tauri::ipc::Request<'_>` / `Response`), not base64 strings. See `commands/audio_files.rs::save_recording`.

## Adding or changing a Tauri command

Touch all relevant layers in one change:

1. `src-tauri/src/commands/<domain>.rs` handler.
2. `src-tauri/src/lib.rs` `invoke_handler![…]` entry with the full path.
3. `src/api.ts` wrapper.
4. `src/types.ts` mirror for changed request/response shapes.
5. `src-tauri/capabilities/default.json` permission when a plugin/API permission is involved.
6. Focused Rust check/test plus frontend typecheck.

## Taste invariants

- Rust edition 2024; use current idioms such as `LazyLock` and `let-else`.
- No comments in code. Prefer clearer names, smaller functions, tests, and docs.
- No `sleep` in scripts; poll with bounded timeouts.
- Prefer boring, inspectable abstractions that agents can reason about from repository-local code.
- Keep platform-specific behaviour explicit and documented in `docs/android.md`, `docs/release.md`, or `docs/technical-debt.md`.

## Mechanical guardrails

Current guardrails live in:

- `.githooks/pre-commit` and `scripts/check-changed.ts` for changed-file checks.
- `cargo xtask check` for the full 11-job local gate.
- `scripts/lint-vue.ts`, `cargo fmt`, clippy, tests, `knip`, `machete`, and audits.

When a prose invariant becomes important enough to repeat, add a lint, test, or xtask check and make its failure message actionable for agents.
