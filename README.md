# WTranscriber

Offline audio transcription app built with Tauri 2, Vue 3, TypeScript, and Rust.
It runs speech recognition locally, adds speaker labels, and can suggest file names with a local LLM.

Main engines:

- ASR: Parakeet, Whisper.cpp, GigaAM
- Diarization: Sortformer ONNX, pyannote + TitaNet ONNX
- Language detection: Silero Lang95 ONNX
- Naming: Qwen3 GGUF via llama.cpp

## Requirements

- [`just`](https://github.com/casey/just)
- Windows: `just setup` installs the rest (Rust, Bun, VS Build Tools, CMake, CUDA, …)
- Linux: Rust 1.88 (`rust-toolchain.toml`), Bun, Tauri platform prerequisites
- Android builds: Android Studio SDK/NDK and JDK 21

Desktop development works on Windows and Linux. `just build` is a Windows shortcut for the full dev release matrix: Windows NSIS, Android APK, and Linux `.deb`. macOS is not in the release matrix.

## Quick start

```bash
just setup             # fresh-clone setup: toolchain (Windows), JS deps, git hooks, cargo prewarm
just dev               # desktop HMR
just android           # clean-start an Android USB HMR session
just check             # full local quality gate
just build             # Windows-only: build releases/dev/
just release           # publish releases/dev/ to the rolling dev prerelease
just release --stable  # bump patch, check, build, publish stable
```

Use `just --list` for all recipes. Developer workflow details live in [`AGENTS.md`](AGENTS.md) and [`docs/dev-loop.md`](docs/dev-loop.md).

## Common tasks

| Need                  | Command                                                                  |
| --------------------- | ------------------------------------------------------------------------ |
| Stop dev sessions     | `just dev stop`                                                          |
| Android APK only      | `bun scripts/android-install.ts`                                         |
| Reinstall Android APK | `bun scripts/android-install.ts --force`                                 |
| Headless emulator     | `bun scripts/android-emu.ts`                                             |
| Changed-file checks   | `just check-changed --staged`                                            |
| Stable release        | `just release --stable`                                                  |
| CLI model catalogue   | `cargo run --manifest-path src-tauri/Cargo.toml --bin wt -- models list` |

## Optional Windows CUDA setup

`just setup` installs or repairs the Windows host toolchain, CUDA Toolkit 12.x, cuDNN 9, and the sherpa-onnx CUDA runtime. The CUDA pieces can also be installed individually:

```powershell
just cudnn         # cuDNN 9 (CUDA 12)
just sherpa-cuda   # sherpa-onnx CUDA runtime
just doctor        # verify prerequisites
```

CUDA is optional. CPU builds and Android builds still work without a CUDA GPU.

## CLI

After a release install, use `wt`. During development, run it through Cargo:

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin wt -- audio.wav
cargo run --manifest-path src-tauri/Cargo.toml --bin wt -- -l en --speakers 3 meeting.ogg
cargo run --manifest-path src-tauri/Cargo.toml --bin wt -- --no-diarize a.wav b.mp3
cargo run --manifest-path src-tauri/Cargo.toml --bin wt -- --device cpu --no-cache a.wav
cargo run --manifest-path src-tauri/Cargo.toml --bin wt -- models list
cargo run --manifest-path src-tauri/Cargo.toml --bin wt -- models install whisper-cpp-large-v3-turbo-q8
cargo run --manifest-path src-tauri/Cargo.toml --bin wt -- models status parakeet-tdt-0.6b-v3-int8
```

The CLI writes JSON transcripts next to the input files.

## Downloads and disk use

The installer is small. Models and native runtimes are downloaded on first use into the OS user data directory:

- Windows: `%APPDATA%\asolopovas\wtranscriber\`
- Linux: `~/.local/share/wtranscriber/`
- Android: app-private storage

Default essentials are about **1.6 GB** before any desktop runtime downloads:

| Component          | ID                          |   Size |
| ------------------ | --------------------------- | -----: |
| ASR                | `parakeet-tdt-0.6b-v3-int8` | 670 MB |
| Language detection | `silero-lang95-onnx`        |  17 MB |
| Diarization        | `sortformer-v2-onnx-4spk`   | 492 MB |
| Local rename LLM   | `qwen3-0.6b-q4km`           | 397 MB |

Desktop CUDA runs also download CUDA runtimes such as sherpa-onnx and cuDNN. The local rename feature downloads a separate llama.cpp runtime. Download archives are kept in cache so reinstalls are faster.

## Model IDs

ASR:

- `parakeet-tdt-0.6b-v3-int8` — default, 25 European languages, Android-friendly
- `whisper-cpp-large-v3-turbo-q8` — multilingual Whisper fallback
- `gigaam-v3-ru` — Russian-specialised model

Diarization:

- `sortformer-v2-onnx-4spk` — default, up to 4 speakers
- `sherpa-pyannote-titanet` — fallback for other cases

LLM:

- `qwen3-0.6b-q4km` — default rename model
- `qwen3-1.7b-q4km` — larger rename model

Run `wt models list` for install status and exact sizes.

## More docs

- [`docs/dev-loop.md`](docs/dev-loop.md) — desktop and Android development loop
- [`docs/android.md`](docs/android.md) — Android prerequisites and bootstrap contract
- [`docs/asr-pipeline-v2.md`](docs/asr-pipeline-v2.md) — current ASR and diarization routing
- [`docs/release.md`](docs/release.md) — release commands, artifacts, signing, recovery
- [`docs/tmp.md`](docs/tmp.md) — scratch files and liveness logs

## License

MIT
