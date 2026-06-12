# ASR model refresh (June 2026)

Status: completed
Owner: agent
Started: 2026-06-12
Related docs: docs/asr-pipeline-v2.md

## Goal

Bring the ASR stack up to the June 2026 state of the art without disturbing the defaults that already work: bump the sherpa-onnx runtime for the diarization crash fix, close the Asian-language gap with Qwen3-ASR, and evaluate Cohere Transcribe as an optional high-accuracy desktop model.

## Current context

- Catalogue (`src-tauri/src/models/catalog_data.rs`): parakeet-tdt-0.6b-v3-int8 (default, 25 EU langs), gigaam-v3-ru, whisper-cpp-large-v3-turbo-q8 (fallback for everything else), sortformer diarizer (default-active).
- Parakeet v3, GigaAM v3 and whisper large-v3-turbo are still the latest in their lines; no replacement needed.
- sherpa-onnx native libs pinned at v1.13.0 (`src-tauri/sherpa-version.txt`); Rust crate at 1.13.1. Upstream v1.13.1 fixed a SIGSEGV in speaker diarization; v1.13.2 is current.
- sherpa-onnx supports Qwen3-ASR since 1.12.34 (int8, hotwords) and Cohere Transcribe since 1.12.35.
- Auto-routing (`docs/asr-pipeline-v2.md`): ru → GigaAM, Parakeet langs → Parakeet, everything else → Whisper. Asian languages currently get the slowest path.

## Acceptance criteria

- sherpa-onnx runtime v1.13.2 across desktop, Android prebuilts, CUDA provider install and bundled runtime scripts; full check matrix green; diarized transcription smoke run passes on desktop.
- Qwen3-ASR 0.6B int8 installable from the model catalogue, dispatched through the sherpa engine path, and auto-routing prefers it over Whisper for its 30 supported languages.
- Decision recorded for Cohere Transcribe (add desktop-only entry or defer) backed by a WER/RTF comparison against whisper-large-v3-turbo on a known-language sample.

## Steps

- [x] Phase 1 — runtime bump to v1.13.2
  - [x] Update `src-tauri/sherpa-version.txt`; replace the hardcoded `v1.13.0` in `scripts/install-windows-runtime.ps1:215`; confirm `install-sherpa-cuda.ps1` derives from the version file.
  - [x] Bump the `sherpa-onnx` crate to the matching release if published.
  - [x] Delete `.android-prebuilt` so xtask re-fetches v1.13.2 prebuilts; rebuild APK and confirm the jniLibs set is complete.
  - [x] `just check`, then a desktop transcription run with sortformer diarization enabled.
- [x] Phase 2 — Qwen3-ASR 0.6B catalogue entry
  - [x] Verify the Rust binding exposes the Qwen3-ASR recogniser config; if not, decide between binding upgrade and the `wt` subprocess path.
  - [x] Pin csukuangfj int8 export URLs + sha256 in `catalog_data.rs`; new engine tag wired through `engine/processor.rs`, `config.rs`, `api.rs`, `wt.rs`, `types.ts`.
  - [x] Extend auto-route: Qwen3 languages (minus Parakeet/GigaAM overlap) → qwen3-asr before Whisper.
  - [x] CLI smoke runs: zh/ja/ko sample plus an EU-language sample proving Parakeet still wins routing.
- [x] Phase 3 — Cohere Transcribe evaluation (decision gate, not a commitment)
  - [x] Run `sherpa-onnx-cohere-transcribe-14-lang` int8 (~2 GB) via CLI on the same samples; compare WER and RTF against whisper-large-v3-turbo-q8.
  - [x] If clearly better: add as `desktop_only` non-default entry; otherwise record the numbers and close.

## Decisions

- 2026-06-12: Keep Parakeet v3 default, GigaAM v3 for Russian, Whisper as long-tail fallback — research found no successor to any of them.
- 2026-06-12: Qwen3-ASR runs both in-process (crate 1.13.2 exposes `OfflineQwen3ASRModelConfig`) and via the `sherpa-onnx-offline` subprocess (`--qwen3-asr-*` flags); model source is the csukuangfj2 int8 export with the tokenizer directory passed as `tokenizer`.
- 2026-06-12: Cohere Transcribe deferred. Its int8 export is ~2.9 GB (not ~2 GB), its 14 languages are a strict subset of Qwen3-ASR's 30, it requires an explicit language (no LID), emits no word timestamps, and its CPU decode speed (sherpa RTF 0.65 on ja, 0.60 on en, 8 threads) is on par with Qwen3 (0.75). Transcript quality on the ja/en samples was identical to Qwen3 and whisper-turbo. Revisit only if a measurably lower WER on real workloads matters more than 3× the disk.
- 2026-06-12: Phases ordered by risk: runtime bump is low-risk and fixes a shipped crash class; Qwen3-ASR is additive; Cohere is gated on measured benefit because of its 2 GB footprint.

## Verification log

- 2026-06-12: `just check` — 11 jobs green after bumping sherpa-version.txt and sherpa-onnx crates to 1.13.2 (first run failed: offline clippy needed `cargo fetch` for the new crates).
- 2026-06-12: `wt --no-cache --diarizer sortformer-onnx tmp/gpu-test.wav` (sherpa-shared build) — parakeet + sortformer end-to-end on 1.13.2, 1 speaker, 2 segments, transcript JSON written.
- 2026-06-12: `xtask release --dev --no-host --no-deb` — Android prebuilts re-fetched as sherpa-onnx-v1.13.2-android.tar.bz2; dev APK contains all seven native libs with the v1.13.2 sherpa set and a rebuilt libwtranscriber_lib.so.
- 2026-06-12: `wt models install qwen3-asr-0.6b-int8` — all six files downloaded with checksums; `wt --lang ja qwen3-ja1.wav` auto-routed to qwen3-asr (subprocess, rtf 0.26 cpu) and matched the reference transcript bar one token; `wt --lang en` still routes to parakeet; 208 Rust tests green.
- 2026-06-12: Cohere eval via `sherpa-onnx-offline` v1.13.2 cpu — ja: exact reference match, RTF 0.648; en: exact match, RTF 0.604; qwen3 same harness ja: exact match, RTF 0.751; whisper-turbo (debug in-process cpu) ja: exact match, 94 s. Both Cohere and Qwen3 return empty `timestamps`/`words`.

## Handoff notes
