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
just dev          # run app in dev mode
just build        # production bundle
just fmt          # format Rust + frontend
just lint         # clippy (warnings as errors) + vue-tsc
just test         # cargo test
```

Run `just` with no arguments to list all recipes.

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
