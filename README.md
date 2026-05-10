# WTranscriber

Audio transcription desktop app. [Tauri 2](https://tauri.app) · Vue 3 + TS · Rust. ASR via sherpa-onnx; diarization via NeMo Sortformer / pyannote.

## Prerequisites

- Rust stable (MSRV 1.85, enforced via `rust-version` in `Cargo.toml`)
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
just build        # bundle desktop app
just build-cli    # build the headless `wt` CLI
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
just build-cpu       # CPU-only full build (sherpa-static feature)
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

## License

MIT
