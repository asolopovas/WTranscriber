# Documentation map

| Topic                          | Document                                   | Verifying source                                          |
| ------------------------------ | ------------------------------------------ | --------------------------------------------------------- |
| Layout and IPC                 | [architecture.md](architecture.md)         | `src/api.ts`, `src/schemas.ts`, Rust command wiring       |
| Recording storage and editing  | [recordings.md](recordings.md)             | Audio/transcription commands, transcript editor tests     |
| Commands and HMR               | [dev-loop.md](dev-loop.md)                 | `justfile`, `scripts/run.ts`, Android bootstrap           |
| Hooks and checks               | [verification.md](verification.md)         | `.githooks/pre-commit`, `scripts/check-changed.ts`, xtask |
| Android                        | [android.md](android.md)                   | `xtask/src/android/`, generated Android project           |
| Install and release            | [release.md](release.md)                   | `scripts/install-*`, `xtask/src/release/`, release config |
| Scratch files                  | [tmp.md](tmp.md)                           | Run harness, bootstrap, `scripts/clean-temp.ts`           |
| Native build/cache constraints | [rust-build-speed.md](rust-build-speed.md) | Cargo profiles, `.cargo/config.toml`, build/check scripts |
| ASR and diarization            | [asr-pipeline.md](asr-pipeline.md)         | Transcriber, engine, diarizer, model catalogue            |
| WhisperX and Community-1       | [quality-pipeline.md](quality-pipeline.md) | `src-tauri/python/`, native quality worker                |
| Guardrail gaps                 | [quality.md](quality.md)                   | Checks and current implementation                         |
| Compatibility pins and patches | [technical-debt.md](technical-debt.md)     | Manifests, lockfiles, referenced platform code            |
| Execution plans                | [plans/README.md](plans/README.md)         | `scripts/lint-docs.ts`                                    |

`bun run lint-docs` checks catalogue/file links, anchors, agent-guide length, and plan shape. Update the owning topic when its behaviour changes; keep generic language/framework tutorials outside this repository.
