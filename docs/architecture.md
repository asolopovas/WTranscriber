# Architecture

Tauri app. Vue/WebView owns presentation; Rust owns filesystem, models, native runtimes, audio processing, and long-running transcription work.

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
- `src/types.ts` mirrors Rust structs. `src/schemas.ts` mirrors the same IPC shapes as Zod schemas so `src/api.ts` can parse untrusted command/event payloads at the boundary. Keep all three synchronized with command return types.
- Use frontend aliases `@/`, `@components/`, `@composables/`, `@utils/`, `@styles/`.
- Capability permissions are least-privilege. If IPC fails, inspect console plus `RustStdoutStderr` before widening permissions.
- Large binary payloads cross IPC as raw bodies (`tauri::ipc::Request<'_>` / `Response`), not base64 strings. See `commands/audio_files.rs::save_recording`.

## Adding or changing a Tauri command

Touch all relevant layers in one change:

1. `src-tauri/src/commands/<domain>.rs` handler.
2. `src-tauri/src/lib.rs` `invoke_handler![…]` entry with the full path.
3. `src/api.ts` wrapper.
4. `src/types.ts` mirror for changed request/response shapes.
5. `src/schemas.ts` Zod schema update for changed request/response/event shapes.
6. `src-tauri/capabilities/default.json` permission when a plugin/API permission is involved.
7. Focused Rust check/test plus frontend typecheck.

## Taste invariants

Audio trim selections and probed duration persist as JSON in the recording folder's `.meta/` subdirectory (`<audio filename>.wtmeta.json`). These files are user metadata, not disposable cache. Legacy sidecars beside recordings remain readable and move into `.meta/` when saved or renamed. The trim editor restores these selections when reopened.

Changing the trim target clears the previous waveform, range, and playhead before loading metadata. Folder listings preload up to 32 waveforms sequentially while transcription is idle, sharing in-flight requests with the editor. The native cache checks source size and modification time on every open, and decoding happens outside its lock so background work cannot block an already cached waveform. Only peaks remain in memory; full recordings are not preloaded into the WebView. Desktop rows provide a trim shortcut beside the AI action; the overflow menu remains available on mobile.

Trim-editor saves are non-destructive. Duration probes and trim changes update their own fields under a shared lock, so probing cannot replace a newer selection. Importing an audio file also copies its trim metadata. The explicit native permanent-trim command remains separate from the editor.

Completed transcripts persist as `.meta/<audio filename>.transcript.json`, with `<audio filename>.txt` beside the recording. Completion, re-diarization, segment edits, and speaker renames synchronise the text file. The local JSON remains available after clearing the disposable transcript cache. Recording renames move both files and trim metadata. Transcript playback uses original-recording timestamps; edited text keeps the segment's original time range rather than inventing word timings.

Fragment playback reads only the requested time range from a native-decoded mono PCM WAV cache, returning a standalone WAV through binary IPC. The WebView keeps at most eight fragments within an 8 MiB cache, preloads the first and next segments, and waits for `canplay` before starting at fragment time zero. Changing source size or modification time remounts the player; the native cache key also tracks these values. Native decoding publishes complete files atomically to prevent playback from reading a partially written cache file. Media failures retain the decoder's error code and message for diagnosis.

Segment playback uses exactly the saved start and end times, clamped to the recording, with no leading or trailing padding. Whisper's token-timestamp option alone can still allocate leading silence to a word; timing-sensitive re-transcription should be checked against the audio rather than inferred from the option name.

Transcript speaker editing offers all segments for that speaker or one selected segment. Segment-only corrections update matching word labels and the distinct speaker count without changing other segments. Find and replace applies literal, case-sensitive text replacements across the current transcript with a match count and preview, preserving speaker labels and segment timestamps. Both operations use the shared edit lock and synchronise durable JSON, cached results when present, and the adjacent `.txt` export.

- Rust edition 2024; use current idioms such as `LazyLock` and `let-else`.
- No comments in code. Prefer clearer names, smaller functions, tests, and docs.
- No `sleep` in scripts; poll with bounded timeouts.
- Parse data at process boundaries. Use Zod on frontend IPC/event boundaries and serde types on Rust boundaries.
- Keep platform-specific behaviour explicit and documented in `docs/android.md`, `docs/release.md`, or `docs/technical-debt.md`.

## Mechanical guardrails

- `.githooks/pre-commit` and `scripts/check-changed.ts` for changed-file checks.
- `cargo xtask check` for the full local gate (jobs enumerated in `docs/verification.md`).
- `scripts/lint-vue.ts`, `cargo fmt`, clippy, tests, `knip`, `machete`, and audits.

When a prose invariant becomes important enough to repeat, add a lint, test, or xtask check with an actionable failure message rather than another doc line.
