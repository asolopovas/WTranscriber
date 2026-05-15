# ASR and diarization pipeline

This document describes the current pipeline. It is not a migration plan.

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
   - `ru` → `gigaam-v3-ru`, then Parakeet, then Whisper.cpp
   - Parakeet's 25 European languages → Parakeet, then Whisper.cpp
   - all other languages → Whisper.cpp
4. Only installed models are selected. If no candidate is installed, the saved config remains unchanged.

Parakeet languages: `bg`, `hr`, `cs`, `da`, `nl`, `en`, `et`, `fi`, `fr`, `de`, `el`, `hu`, `it`, `lv`, `lt`, `mt`, `pl`, `pt`, `ro`, `sk`, `sl`, `es`, `sv`, `ru`, `uk`.

## Engines

| Engine tag    | Models                          | Use case                |
| ------------- | ------------------------------- | ----------------------- |
| `parakeet`    | `parakeet-tdt-0.6b-v3-int8`     | Fast default ASR        |
| `nemo-ctc`    | `gigaam-v3-ru`                  | Russian-specialised ASR |
| `whisper-cpp` | `whisper-cpp-large-v3-turbo-q8` | Multilingual fallback   |

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
