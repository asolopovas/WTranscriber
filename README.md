# WTranscriber

Audio transcription desktop app. [Tauri 2](https://tauri.app) · Vue 3 + TS · Rust. ASR via sherpa-onnx; diarization via NeMo Sortformer / pyannote.

## Prerequisites

- Rust stable (MSRV 1.88, enforced via `rust-version` in `Cargo.toml` and pinned in `rust-toolchain.toml`)
- Bun
- [`just`](https://github.com/casey/just)
- Tauri platform toolchain ([docs](https://tauri.app/start/prerequisites/))

Desktop dev (`just dev`, `just build`) is exercised on Linux and Windows.
macOS builds via `tauri build` (bundle target `app`) but is not part of the
release matrix; expect rough edges. Android dev from a macOS host works.

## Quick start

```bash
just setup        # install JS deps + git hooks
just dev          # desktop HMR
just build        # full release matrix (Linux + Windows + Android), CLI included
just build-host   # current host only: GUI installer + wt CLI
just cli ARGS     # run the CLI in dev (e.g. `just cli models list`)
just check        # parallel pre-release gate
```

`just --list` for everything else. Conventions and dev-loop rules: [`AGENTS.md`](AGENTS.md).

## CUDA build (Windows, optional)

```powershell
just sherpa-cuda     # prebuilt sherpa-onnx CUDA archive
just cudnn           # cuDNN 9 for CUDA 12
just nemo-deps       # Python venv + nemo_toolkit for Sortformer diarization
just dev-cpu         # HMR with sherpa-static (no CUDA toolchain required)
just build-host      # Full bundle (NSIS / .deb); sherpa-static + sidecar GPU
```

Requires NVIDIA CUDA 12.x (`cudart64_12.dll`) installed system-wide. The sidecar `sherpa-onnx-offline.exe` runs CUDA independently of the build feature; toggling Device → GPU in the UI uses the sidecar with automatic CPU fallback.

## CLI

```bash
wt audio.wav                       # transcribe → audio_<model>_<ts>.json
wt -l en --speakers 3 meeting.ogg
wt --no-diarize a.wav b.mp3
wt --device cuda --no-cache a.wav
wt models list
wt models install sherpa-whisper-turbo
wt models status sherpa-pyannote-titanet
```

## Disk usage

On first launch the app downloads runtimes and models into the OS user dirs (`%APPDATA%\asolopovas\wtranscriber\` on Windows, `~/.local/share/wtranscriber/` on Linux). The installer itself is small (~24 MB Windows NSIS, ~38 MB Linux `.deb`, ~42 MB Android `.apk`); the bulk lands on first run.

### Default essentials (first-run automatic downloads)

Desktop, NVIDIA GPU host:

| Component                                 |    Download | Notes                                |
| ----------------------------------------- | ----------: | ------------------------------------ |
| Whisper large-v3-turbo (ASR)              |      989 MB | `sherpa-whisper-turbo`, 99 languages |
| Sortformer 4-speaker v2.1 ONNX (diarizer) |      470 MB | `sortformer-v2-onnx-4spk`            |
| Qwen3 0.6B Q4_K_M (LLM, auto-rename)      |      378 MB | `qwen3-0.6b-q4km`                    |
| sherpa-onnx CUDA runtime (Windows)        |      296 MB | extracts to ~700 MB                  |
| cuDNN 9.21.1.3 (Windows)                  |      646 MB | extracts to ~1.0 GB                  |
| llama.cpp b9045 (Windows)                 |       15 MB | naming engine                        |
| **Total bandwidth**                       | **~2.8 GB** |                                      |
| **Total disk after install**              | **~3.7 GB** | runtimes are extracted               |

Android default essentials (Parakeet + TitaNet + Qwen3 0.6B) total **~1.1 GB**; the APK already bundles its native libs so there's no separate runtime download.

### Optional ASR models

| ID                          |   Size | Languages                     |
| --------------------------- | -----: | ----------------------------- |
| `sherpa-whisper-turbo`      | 989 MB | 99 (multilingual)             |
| `parakeet-tdt-0.6b-v3-int8` | 640 MB | 25 EU langs (Android default) |
| `gigaam-v3-ru`              | 214 MB | Russian-only                  |

### Optional LLM models (auto-rename)

| ID                          |    Size |
| --------------------------- | ------: |
| `qwen3-0.6b-q4km` (default) |  378 MB |
| `qwen3-1.7b-q4km`           | 1.03 GB |

### Optional diarizers

| ID                                  |   Size | Notes                                     |
| ----------------------------------- | -----: | ----------------------------------------- |
| `sortformer-v2-onnx-4spk` (default) | 470 MB | ≤4 speakers, GPU-accelerated              |
| `sherpa-pyannote-titanet`           | 102 MB | fallback, no GPU needed (Android default) |
| `nemo-sortformer-v2`                |  ~5 GB | Python + PyTorch runtime, legacy          |

### Runtime archives (host downloads, auto-installed)

| Component           | Windows x64 | Linux x64 |
| ------------------- | ----------: | --------: |
| sherpa-onnx (CPU)   |       17 MB |     26 MB |
| sherpa-onnx (CUDA)  |      296 MB |    191 MB |
| cuDNN 9 (CUDA 12)   |      646 MB |    925 MB |
| llama.cpp b9045     |       15 MB |     14 MB |
| NeMo Python runtime |      ~5 GB¹ |    ~5 GB² |

¹ Windows: install manually via `scripts/install-nemo-deps.ps1` (or `just nemo-deps`). Auto-install is Linux-only. <br>
² Linux: auto-installed in the background when the NeMo diarizer is selected on a GPU host. Installs `nemo_toolkit[asr]` into an isolated `uv`-managed Python 3.12 (torch + CUDA wheels + nemo_toolkit).

The desktop build links sherpa-onnx-cpu statically when configured with `sherpa-static`, so CPU-only users skip the sherpa runtime download entirely.

## License

MIT
