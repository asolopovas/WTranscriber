# ASR and diarization pipeline

## Execution stages

`transcriber/job.rs` drives every transcription (GUI and `wt` CLI):

1. Cache probe — key over source mtime, model, language, speakers, trim, timestamp mode (`transcriber/cache.rs`); hit serves immediately.
2. Slab streaming — `audio_toolkit/stream.rs` decodes via ffmpeg/symphonia into ~60 s slabs (10 s calibration first slab; durations in `transcriber/job/slab.rs`). Slab ends snap to the lowest-energy point within ±1.5 s of the nominal boundary (`SNAP_SEARCH_SEC`).
3. VAD gate — no-speech slabs are skipped before any engine runs (Silero VAD, fail-open if the model is absent; `WT_NO_VAD_GATE=1` disables; `job/streaming.rs`). Speech-bearing chunks retain their complete audio because hard speech-span cuts dropped quiet words in the real-recording check.
4. Engine dispatch per slab (`engine/whisper_cpp.rs`, `engine/processor.rs`); `engine::resolve_device` (`engine/mod.rs`) gives CLI and GUI the same cuda-fallback decision:
   - whisper-cpp + device=cuda → `wt-whisper-cuda-worker.exe` in persistent `--serve` mode (model loaded once per job); falls back to one-shot spawn per slab when the serve worker is absent.
   - whisper-cpp + cpu → in-process whisper-rs.
   - sherpa engines (parakeet, gigaam, qwen3-asr) → in-process with the resolved ONNX provider (`runtimes/dependencies.rs`). Unsupported CUDA requests change the job device to CPU before dispatch and emit a `transcribe:warning` event; they must not select an external executable merely because the original request was CUDA. Explicit `WT_USE_SUBPROCESS=1` remains available for subprocess diagnostics.
   - Whisper word-timestamp mode emits one token per segment; downstream merge relies on that granularity. Diarization always requests word timings, even when the optional timing setting is off.
5. Dedup — per-segment and cross-segment token collapse against whisper repetition loops (`job/postprocess.rs`, `dedup.rs`).
6. Partial save/resume — atomic per-slab snapshots (`transcriber/partial.rs`); resume skips below `resume_floor`.
7. All diarizers receive only the saved trim. Native Sortformer/TitaNet use an isolated cropped WAV; results are clamped to that audio and shifted to source time exactly once. Re-diarization follows the same path and excludes saved words outside the current trim. Diarization + merge — per-word or per-segment speaker lookup, speaker-preserving sentence grouping (`transcriber/transcript/`).
8. Cache store, durable `.meta` transcript, and adjacent text export.

Speaker assignment preserves short responses and alternating turns. Overlapping turns for one speaker are counted once; exact ties use a stable speaker order or the previous label, without overriding a stronger match. Words outside detected speech turns remain unassigned. Word grouping preserves dialogue order and keeps the furthest word end when timestamps overlap. Neither initial transcription nor re-diarization smooths away isolated speaker turns. Sentence grouping also splits across long timestamp gaps. Speech detection is probabilistic and word times remain model estimates, not forced alignment. Previously saved results remain unchanged until explicitly reprocessed.

Postprocessing also handles phrase-sized Whisper tokens and tokenless segment loops. Three or more low-confidence copies within 30 seconds can collapse to the first copy, retaining intervening text; isolated and distant repetitions remain. This is a heuristic, not a claim about what was spoken. Previously saved transcripts are kept intact, and the revised cache-key version forces a fresh run when transcription is explicitly requested again.

Sherpa/Parakeet token durations, when supplied, determine word ends. Subword durations are merged into the word span; the next word's start is only a fallback when measured durations are unavailable. Final word ranges are clamped to recording duration. This prevents sentence playback from including a long silent gap after its last word. CLI JSON exports live under the source folder's `.meta/` directory. `--no-cache` bypasses durable and disposable transcript caches, clears matching partial work, and keeps the previous saved transcript until its replacement succeeds.

Diarization receives the same requested/resolved device. Sortformer selects CUDA in CUDA builds and honours explicit CPU selection; TitaNet selects the supported ONNX provider for both segmentation and embedding. GPU initialisation errors retry on CPU before reporting failure. Small VAD and language probes stay on CPU to avoid unnecessary GPU transfers and session overhead.

Thread cap: GPU decode caps engine threads at 2 (`engine/runtime.rs`), keyed on the resolved provider; CPU paths use the requested count (default 4). Engine warnings reach the UI through `progress::Sink::warn` → `transcribe:warning`.

## GUI queue ownership

Manual transcription, folder/selection batches, and re-diarization share `useTranscriptionQueue`. Enqueueing appends jobs and ignores paths already pending; it never resets active work. Each job captures its settings when enqueued. Pending files are immediately busy, so repeated clicks cannot create duplicate work. The queue records `queued`, `running`, `cancelling`, and terminal `succeeded`, `failed`, or `cancelled` states. A failed job does not prevent later jobs from starting.

Rust's `commands/transcription_queue.rs` independently bounds native execution to one job across transcription and re-diarization. This protects shared engine caches, subprocess ownership, and model memory. The worker owns its cancellation registration and execution guard until the native operation has actually finished, even if its IPC caller disappears. Cancelling pending work removes it without waiting for the active job; cancelling active work signals only that job and retains its slot while the worker shuts down. `cancel_transcribe` is scoped to one path; `cancel_all_transcribes` remains an explicit backend operation.

Queue tests use controlled completion signals. Browser tests cover additive admission and cancellation/failure isolation through the UI with mocked IPC; they do not verify native inference.

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

`--model` determines the engine from the catalogue; `--engine` is diagnostic. `--no-auto-route` keeps the saved model/language. `--diarizer` accepts `sortformer-onnx` or `titanet`; `--speakers N` supplies the expected count when diarization is enabled. Other syntax is in `wt --help`.

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

Supported language lists and catalogue entries are owned by `api.rs::route_model_for_lang` and `models/catalog_data.rs` rather than copied here.

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

## Verification

Routing/merge changes need focused native tests and a no-cache CLI run through the affected model/language/diarizer. Inspect saved language, speaker count, words, and timestamps against the source audio. Durable storage and edit contracts are in [recordings.md](recordings.md).

## Optional desktop alignment and speaker pipeline

The [WhisperX + Community-1 pipeline](quality-pipeline.md) keeps the selected native ASR model, adds forced word alignment and uses Community-1 exclusive speaker turns. It also supports realigning existing transcripts during re-diarization.

Completed recordings expose **Retranscribe** on the row action and overflow menu. It captures current settings, bypasses transcript and partial caches, backs up the saved result, and transcribes the saved trim again. Ordinary transcription may still reuse a matching cache.
