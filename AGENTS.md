# WTranscriber agent guide

Tauri 2, Rust, Vue 3/TypeScript, Vite, Bun, and `just`. Rust is pinned in `rust-toolchain.toml`; dependency constraints live in [technical debt](docs/technical-debt.md).

## Project constraints

- No code comments. No sleeps in scripts; use bounded polling.
- Conventional commits and simple British English.
- The pre-commit hook is mandatory; its release-bump exception and checks are in [verification](docs/verification.md).
- Vue owns presentation; Rust owns native work. IPC types, schemas, command registration, and capabilities follow [architecture](docs/architecture.md).
- During Android HMR, APK/release builds must not replace the debug app. Restart bootstrap after native/config/capability edits; use the [dev-loop signals](docs/dev-loop.md#live-session-signals).

## Documentation

| Need                                     | Source                                                                               |
| ---------------------------------------- | ------------------------------------------------------------------------------------ |
| Documentation map                        | [docs/README.md](docs/README.md)                                                     |
| Layout and IPC                           | [docs/architecture.md](docs/architecture.md)                                         |
| Recording metadata, playback, and edits  | [docs/recordings.md](docs/recordings.md)                                             |
| Commands and HMR                         | [docs/dev-loop.md](docs/dev-loop.md)                                                 |
| Checks and hook contract                 | [docs/verification.md](docs/verification.md)                                         |
| Android bootstrap and WebView inspection | [docs/android.md](docs/android.md)                                                   |
| Install, release, signing, Windows VM    | [docs/release.md](docs/release.md)                                                   |
| Scratch files                            | [docs/tmp.md](docs/tmp.md)                                                           |
| Native build/cache constraints           | [docs/rust-build-speed.md](docs/rust-build-speed.md)                                 |
| Transcription and diarization            | [docs/asr-pipeline.md](docs/asr-pipeline.md)                                         |
| Optional WhisperX/Community-1            | [docs/quality-pipeline.md](docs/quality-pipeline.md)                                 |
| Guardrail gaps and dependency pins       | [docs/quality.md](docs/quality.md), [docs/technical-debt.md](docs/technical-debt.md) |
| Execution-plan format                    | [docs/plans/README.md](docs/plans/README.md)                                         |

## Commands

| Task                   | Command                                  |
| ---------------------- | ---------------------------------------- |
| Desktop HMR / stop     | `just dev` / `just dev stop`             |
| Android HMR            | `just android`                           |
| Checks                 | `just check` / `just check-changed`      |
| Local install          | `just install`                           |
| Windows release matrix | `just build`                             |
| Publish dev / stable   | `just release` / `just release --stable` |
| Prerequisites          | `just setup` / `just doctor`             |

Use `just --list` for additional recipes. Project-specific knowledge lives in the linked docs; general Rust/Tauri guidance comes from installed help and primary documentation.
