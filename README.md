# WTranscriber

Offline transcription app for Windows, Linux, and Android, built with Tauri 2, Vue 3, TypeScript, and Rust. Speech recognition, speaker labels, language detection, and optional filename suggestions run locally.

## Install from a checkout

```bash
just install
```

Requires [just](https://github.com/casey/just). Installation bootstraps missing tools and builds the current checkout, including local edits; it does not fetch updates or change branches. Fresh setup requires internet access and may need administrator privileges.

Windows uses NSIS; `just install --interactive` shows its UI. Linux installs binaries under `~/.local/bin`, libraries under `~/.local/lib/wtranscriber`, and a desktop launcher under `${XDG_DATA_HOME:-~/.local/share}`. `WT_INSTALL_PREFIX` changes the Linux binary/library prefix. Linux prerequisite setup supports apt, dnf, pacman, and zypper on x86_64/aarch64. Other distributions need the native prerequisites installed manually. macOS is unsupported.

CUDA is selected automatically when the GPU/toolkit supports it. `WT_CUDA=0 just install` selects CPU; `WT_CUDA=1 just install` requires CUDA. See [development](docs/dev-loop.md#linux-acceleration) and [installation details](docs/release.md#installing-a-branch-locally).

## Development

| Task                        | Command                 |
| --------------------------- | ----------------------- |
| Fresh-clone setup           | `just setup`            |
| Desktop HMR                 | `just dev`              |
| Android USB HMR             | `just android`          |
| Stop dev sessions           | `just dev stop`         |
| Checks                      | `just check`            |
| Windows release matrix      | `just build`            |
| Publish rolling dev release | `just release`          |
| Stable patch release        | `just release --stable` |

Use `just --list` for all recipes. Android needs the pinned SDK/NDK and JDK 21; see [Android](docs/android.md). Release commands publish externally; their exact build/signing contracts are in [release.md](docs/release.md).

## CLI

After installation, use `wt`. During development, prefix these commands with `cargo run --manifest-path src-tauri/Cargo.toml --bin wt --`:

```bash
wt audio.wav
wt --lang en --speakers 3 meeting.ogg
wt --no-diarize a.wav b.mp3
wt --device cpu --no-cache a.wav
wt models list
wt models install whisper-cpp-large-v3-turbo-q8
```

JSON transcripts are written under each input folder's `.meta/`; the text export sits beside the recording. `wt --help` provides CLI options, and [ASR routing](docs/asr-pipeline.md) explains model/device selection.

## Models and downloads

Models and native runtimes download on first use into `%APPDATA%\asolopovas\wtranscriber\` on Windows, `~/.local/share/wtranscriber/` on Linux, or app-private Android storage. Download archives remain cached for reinstalls. Default models total about 1.6 GB before desktop runtimes.

| Role                 | Default model               |
| -------------------- | --------------------------- |
| ASR                  | `parakeet-tdt-0.6b-v3-int8` |
| Language detection   | `silero-lang95-onnx`        |
| Diarization          | `sortformer-v2-onnx-4spk`   |
| Filename suggestions | `qwen3-0.6b-q4km`           |

Whisper.cpp, GigaAM, Qwen3-ASR, and TitaNet provide other recognition/speaker options. `wt models list` shows the current catalogue, sizes, and installation status. Desktop users can add [WhisperX alignment and Community-1](docs/quality-pipeline.md).

[Documentation map](docs/README.md) · [Agent guide](AGENTS.md) · [Security reports](SECURITY.md)

## License

MIT
