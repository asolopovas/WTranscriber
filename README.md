# WTranscriber

Audio transcription desktop application powered by [Tauri](https://tauri.app), [Vue 3](https://vuejs.org), and Rust. Successor to [`wt`](https://github.com/asolopovas/wt) — same engine (sherpa-onnx for ASR, NeMo Sortformer / pyannote for diarization) wrapped in a native cross-platform shell with a modern web UI.

## Stack

- **Shell:** Tauri 2 (Rust)
- **Frontend:** Vue 3 + TypeScript + Vite
- **Package manager:** Bun
- **Task runner:** [`just`](https://github.com/casey/just)

## Prerequisites

- Rust (stable, edition 2021+)
- Bun
- `just`
- Platform toolchain for Tauri ([see docs](https://tauri.app/start/prerequisites/))

## Quick start

```bash
just setup        # install JS deps
just dev          # run desktop app
just build        # bundle desktop app
just build-cli    # build the headless `wt` CLI
just cli ARGS     # run the CLI in dev (e.g. `just cli models list`)
just fmt          # format Rust + frontend
just lint         # clippy (warnings as errors) + vue-tsc
just test         # cargo test
```

Run `just` with no arguments to list all recipes.

## CUDA build (Windows, optional)

The default build links sherpa-onnx statically and runs on CPU. To enable
in-process GPU acceleration:

```powershell
just sherpa-cuda     # downloads prebuilt sherpa-onnx CUDA archive, sets SHERPA_ONNX_LIB_DIR + PATH
just cudnn           # installs cuDNN 9 for CUDA 12
just build-cuda      # tauri build --no-default-features --features cuda
```

Requires NVIDIA CUDA 12.x runtime (`cudart64_12.dll`) installed system-wide
— get it from the [CUDA Toolkit](https://developer.nvidia.com/cuda-downloads).
The sidecar `sherpa-onnx-offline.exe` runs CUDA independently of the build
feature; toggling Device → GPU in the UI will use the sidecar even on a
default static build, with automatic CPU fallback if CUDA fails to load.

## CLI

```bash
wt audio.wav                       # transcribe (writes audio_<model>_<ts>.json)
wt -l en --speakers 3 meeting.ogg  # language hint + speaker count
wt --no-diarize a.wav b.mp3        # multiple files, no diarization
wt --device cuda --no-cache a.wav  # CUDA provider, bypass transcript cache
wt models list                     # show catalog + install state
wt models install sherpa-whisper-turbo
wt models status sherpa-pyannote-titanet
```

## Status

Early stage. Skeleton is in place; transcription pipeline is being ported
from the Go implementation in [`wt`](https://github.com/asolopovas/wt).
See [`AGENTS.md`](AGENTS.md) for the migration map and conventions.

## Layout

```
src/            Vue 3 frontend
src-tauri/      Rust backend (Tauri commands, transcription pipeline)
public/         Static assets
```

## Bundle sizes (Windows x64)

| Artifact                 | Size    |
| ------------------------ | ------- |
| `wtranscriber.exe` (GUI) | ~9.5 MB |
| `wt.exe` (CLI)           | ~5.3 MB |
| NSIS installer           | ~4.4 MB |
| MSI installer            | ~6.7 MB |

ffmpeg, sherpa-onnx and llama.cpp binaries are not bundled — they are
resolved at runtime via `which` / `WT_*_DIR` env vars / exe-adjacent
install paths.

## License

MIT
