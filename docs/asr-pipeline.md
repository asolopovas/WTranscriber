# ASR and diarization pipeline

## Execution stages

`transcriber/job.rs` drives every transcription (GUI and `wt` CLI):

1. Cache probe — key over source mtime, model, language, speakers, trim, timestamp mode (`transcriber/cache.rs`); hit serves immediately.
2. Slab streaming — `audio_toolkit/stream.rs` decodes via ffmpeg/symphonia into ~60 s slabs (10 s calibration first slab; durations in `transcriber/job/slab.rs`). Slab ends snap to the lowest-energy point within ±1.5 s of the nominal boundary (`SNAP_SEARCH_SEC`).
3. VAD gate — no-speech slabs are skipped before any engine runs (silero VAD, fail-open if the model is absent; `WT_NO_VAD_GATE=1` disables; `job/streaming.rs`).
4. Engine dispatch per slab (`engine/whisper_cpp.rs`, `engine/processor.rs`); `engine::resolve_device` (`engine/mod.rs`) gives CLI and GUI the same cuda-fallback decision:
   - whisper-cpp + device=cuda → `wt-whisper-cuda-worker.exe` in persistent `--serve` mode (model loaded once per job); falls back to one-shot spawn per slab when the serve worker is absent.
   - whisper-cpp + cpu → in-process whisper-rs.
   - sherpa engines (parakeet, gigaam, qwen3-asr) → in-process with the `cuda` feature, otherwise `wt` subprocess with the resolved ONNX provider (`runtimes/dependencies.rs`); the directml GUI build resolves cuda → cpu and emits a `transcribe:warning` event.
   - Whisper word-timestamp mode emits one token per segment; downstream merge relies on that granularity.
5. Dedup — per-segment and cross-segment token collapse against whisper repetition loops (`job/postprocess.rs`, `dedup.rs`).
6. Partial save/resume — atomic per-slab snapshots (`transcriber/partial.rs`); resume skips below `resume_floor`.
7. Diarization + merge — per-word speaker lookup, flicker smoothing, sentence grouping (`transcriber/transcript/`).
8. Cache store and JSON export.

Thread cap: GPU decode caps engine threads at 2 (`engine/runtime.rs`), keyed on the resolved provider; CPU paths use the requested count (default 4). Engine warnings reach the UI through `progress::Sink::warn` → `transcribe:warning`.

## GUI queue ownership

Manual transcription, folder/selection batches, and re-diarization share `useTranscriptionQueue`. Enqueueing appends jobs and ignores paths already pending; it never resets active work. Each job captures its settings when enqueued. Pending files are immediately busy, so repeated clicks cannot create duplicate work. The queue records `queued`, `running`, `cancelling`, and terminal `succeeded`, `failed`, or `cancelled` states. A failed job does not prevent later jobs from starting.

Rust's `commands/transcription_queue.rs` independently bounds native execution to one job across transcription and re-diarization. This protects shared engine caches, subprocess ownership, and model memory. The worker owns its cancellation registration and execution guard until the native operation has actually finished, even if its IPC caller disappears. Cancelling pending work removes it without waiting for the active job; cancelling active work signals only that job and retains its slot while the worker shuts down. `cancel_transcribe` is scoped to one path; `cancel_all_transcribes` remains an explicit backend operation.

The previous implementation had separate manual and batch lifecycles, treated a row's Stop action as cancel-all, replaced cancellation tokens for duplicate paths, and released the native lock while cancelled work continued in a detached task. That allowed unrelated progress to be cleared and shared engine shutdown to overlap subsequent jobs. Starting a batch alone did not explicitly request cancellation; the original incident has no matching captured runtime trace, so these verified defects are covered separately by regression tests.

The lifecycle follows the single-owner state transitions and completion guard used in [Handy's recording coordinator](https://github.com/cjpais/Handy/blob/main/src-tauri/src/transcription_coordinator.rs), adapted to independent file jobs. [Tokio's blocking-task contract](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) requires waiting for already-running native work rather than assuming cancellation aborts it. [p-queue](https://github.com/sindresorhus/p-queue#api) provides the comparable additive admission, bounded concurrency and per-task cancellation model; the Vue composable keeps those semantics without another runtime dependency. Queue unit tests use controlled completion signals; the Playwright suite exercises adding remaining files during active transcription and cancellation/failure isolation through the actual UI with mocked IPC.

## Defaults

Fresh installs use these catalogue entries (`models/catalog.rs`):

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

When `--model` is not passed and `--no-auto-route` is not set, the CLI picks the best installed ASR model for the language (`api.rs::route_model_for_lang`).

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
