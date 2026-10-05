# Architecture

Vue/WebView owns presentation; Rust owns files, models, native runtimes, audio processing, and transcription workers.

## Layout

| Path                                                | Responsibility                                                    |
| --------------------------------------------------- | ----------------------------------------------------------------- |
| `src/components/`, `src/composables/`, `src/utils/` | UI, state/effects, frontend utilities                             |
| `src/api.ts`, `src/types.ts`, `src/schemas.ts`      | Typed IPC wrappers and runtime payload validation                 |
| `src-tauri/src/commands/`                           | Domain command handlers                                           |
| `src-tauri/src/models/`, `runtimes/`, `engine/`     | Model metadata/downloads, native runtime management, ASR adapters |
| `src-tauri/src/transcriber/`, `diarizer/`           | Jobs, caches, streaming, transcripts, speakers                    |
| `src-tauri/src/audio/`, `audio_toolkit/`            | Decoding and audio operations                                     |
| `src-tauri/src/llm/`, `namer/`                      | Local LLM and filename suggestions                                |
| `xtask/src/`, `scripts/`                            | Checks, releases, Android orchestration, Bun/Windows helpers      |
| `.vscode/`                                          | Development/check task wrappers                                   |

Rust entrypoints are `src-tauri/src/lib.rs` and `src-tauri/src/bin/wt.rs`. Frontend aliases are `@/`, `@components/`, `@composables/`, `@utils/`, and `@styles/`.

## IPC contract

Frontend native access uses typed Tauri commands/events. Cross-boundary errors use serializable `error::Error`. Keep Rust structs, `src/types.ts`, and `src/schemas.ts` aligned; `src/api.ts` validates command/event payloads.

A command change includes its `commands/<domain>.rs` handler, full-path registration in `lib.rs`'s invoke handler, API wrapper, changed TS types/Zod schemas, and any required plugin/API permission in `src-tauri/capabilities/default.json`. Check the exact IPC error before widening capabilities. Validate through [verification.md](verification.md).

Large binary payloads use raw `tauri::ipc::Request`/`Response`, as in `commands/audio_files.rs::save_recording`.

## Domain contracts

[recordings.md](recordings.md) owns metadata, playback, durable edits, and removal. [asr-pipeline.md](asr-pipeline.md) owns inference routing, timestamps, queue ownership, and cancellation. Platform/build constraints live in [Android](android.md), [release](release.md), and [technical debt](technical-debt.md).
