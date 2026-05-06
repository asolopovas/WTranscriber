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

## License

MIT
