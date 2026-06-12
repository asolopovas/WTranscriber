# ASR and diarization pipeline

This document describes the current pipeline. It is not a migration plan.

## Execution stages

`transcriber/job.rs` drives every transcription (GUI and `wt` CLI):

1. Cache probe — key over source mtime, model, language, speakers, trim, timestamp mode (`transcriber/cache.rs`); hit serves immediately.
2. Slab streaming — `audio_toolkit/stream.rs` decodes via ffmpeg/symphonia into ~60 s slabs (10 s calibration first slab). Slab ends snap to the lowest-energy point within ±1.5 s of the nominal boundary so words are not cut mid-syllable.
3. VAD gate — slabs with no speech (silero VAD, fail-open if the model is absent; `WT_NO_VAD_GATE=1` disables) are skipped before any engine runs (`job/streaming.rs`).
4. Engine dispatch per slab (`engine/whisper_cpp.rs`, `engine/processor.rs`); `engine::resolve_device` gives CLI and GUI the same cuda-fallback decision:
   - whisper-cpp + device=cuda → `wt-whisper-cuda-worker.exe` in persistent `--serve` mode (model loaded once per job); pre-serve workers fall back to one-shot spawning per slab.
   - whisper-cpp + cpu → in-process whisper-rs.
   - sherpa engines (parakeet, gigaam, qwen3-asr) → in-process when the `cuda` feature is on, otherwise `wt` subprocess with the resolved ONNX provider (`runtimes/dependencies.rs`); the directml GUI build resolves cuda → cpu and surfaces it as a `transcribe:warning` event.
   - Whisper word-timestamp mode emits one token per segment; downstream merge relies on that granularity.
5. Dedup — per-segment and cross-segment token collapse (`job/postprocess.rs`, `dedup.rs`) against whisper repetition loops.
6. Partial save/resume — atomic per-slab snapshots (`transcriber/partial.rs`); resume skips below `resume_floor`.
7. Diarization + merge — per-word speaker lookup, flicker smoothing, sentence grouping (`transcript/`).
8. Cache store and JSON export.

Thread cap: GPU decode caps engine threads at 2 (`engine/runtime.rs`), keyed on the resolved provider; CPU paths use the requested count. Engine warnings reach the UI through `Sink::warn` → `transcribe:warning`.

## Defaults

Fresh installs use these catalogue entries:

| Role               | Default ID                  | Notes                             |
| ------------------ | --------------------------- | --------------------------------- |
| ASR                | `parakeet-tdt-0.6b-v3-int8` | 25 European languages, word times |
| Language detection | `silero-lang95-onnx`        | Fast spoken-language probe        |
| Diarization        | `sortformer-v2-onnx-4spk`   | ONNX Sortformer, up to 4 speakers |
| Rename LLM         | `qwen3-0.6b-q4km`           | Local filename suggestions        |

Android uses the same default ASR, diarizer, language detector, and rename model. Non-default models are still download-on-demand.

## CLI controls

```bash
wt audio.wav
wt --lang en --speakers 2 meeting.wav
wt --model whisper-cpp-large-v3-turbo-q8 audio.wav
wt --diarizer sortformer-onnx audio.wav
wt --diarizer titanet --speakers 6 audio.wav
wt --no-diarize audio.wav
wt --no-auto-route audio.wav
```

Important rules:

- `--model` is authoritative. The engine is taken from the model catalogue.
- `--engine` exists for advanced debugging only.
- `--no-auto-route` keeps the saved model and language.
- `--diarizer` accepts `sortformer-onnx` or `titanet`.
- `--speakers N` sets the expected speaker count when diarization is enabled.

## Language-aware ASR routing

When `--model` is not passed and `--no-auto-route` is not set, the CLI chooses the best installed ASR model for the language.

1. If `--lang` or saved `config.language` is a real code, use it.
2. If the language is empty or `auto`, probe the first input with `silero-lang95-onnx`.
3. Route by language:
   - `ru` → `gigaam-v3-ru`, then Parakeet, then Qwen3-ASR, then Whisper.cpp
   - Parakeet languages also covered by Qwen3-ASR → Parakeet, then Qwen3-ASR, then Whisper.cpp
   - remaining Parakeet languages → Parakeet, then Whisper.cpp
   - Qwen3-only languages (`zh`, `yue`, `ar`, `id`, `ko`, `th`, `vi`, `ja`, `tr`, `hi`, `ms`, `fil`, `fa`, `mk`) → Qwen3-ASR, then Whisper.cpp
   - all other languages → Whisper.cpp
4. Only installed models are selected. If no candidate is installed, the saved config remains unchanged.

Parakeet languages: `bg`, `hr`, `cs`, `da`, `nl`, `en`, `et`, `fi`, `fr`, `de`, `el`, `hu`, `it`, `lv`, `lt`, `mt`, `pl`, `pt`, `ro`, `sk`, `sl`, `es`, `sv`, `ru`, `uk`.

Qwen3-ASR languages: `zh`, `en`, `yue`, `ar`, `de`, `fr`, `es`, `pt`, `id`, `it`, `ko`, `ru`, `th`, `vi`, `ja`, `tr`, `hi`, `ms`, `nl`, `sv`, `da`, `fi`, `pl`, `cs`, `fil`, `fa`, `el`, `hu`, `mk`, `ro`.

## Engines

| Engine tag    | Models                          | Use case                      |
| ------------- | ------------------------------- | ----------------------------- |
| `parakeet`    | `parakeet-tdt-0.6b-v3-int8`     | Fast default ASR              |
| `nemo-ctc`    | `gigaam-v3-ru`                  | Russian-specialised ASR       |
| `qwen3-asr`   | `qwen3-asr-0.6b-int8`           | 30 languages incl. Asian/MENA |
| `whisper-cpp` | `whisper-cpp-large-v3-turbo-q8` | Multilingual fallback         |

The current catalogue has no legacy Sherpa Whisper fallback.

## Diarization

| CLI value         | Catalogue ID              | Notes                                   |
| ----------------- | ------------------------- | --------------------------------------- |
| `sortformer-onnx` | `sortformer-v2-onnx-4spk` | Default. Best for up to 4 speakers.     |
| `titanet`         | `sherpa-pyannote-titanet` | ONNX fallback using pyannote + TitaNet. |

Diarization runs without Python. The transcript merge expects word-level or short ASR segments; Parakeet and Whisper.cpp both provide that.

## Verification samples

Use focused CLI runs when changing routing, models, diarization, or transcript merge code:

```bash
wt --no-cache --lang en audio_30s_4speakers.m4a
wt --no-cache --lang ru russian.wav
wt --no-cache --lang zh mandarin.wav
wt --no-cache --model whisper-cpp-large-v3-turbo-q8 --diarizer sortformer-onnx audio.wav
wt --no-cache --diarizer titanet --speakers 6 meeting.wav
```

Expected result: each run produces a JSON transcript with a sensible `language`, `speakers_detected`, utterance list, and word timings.
