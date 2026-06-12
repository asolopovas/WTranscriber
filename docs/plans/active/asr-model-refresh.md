# ASR model refresh (June 2026)

Status: active
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
- [ ] Phase 2 — Qwen3-ASR 0.6B catalogue entry
  - [ ] Verify the Rust binding exposes the Qwen3-ASR recogniser config; if not, decide between binding upgrade and the `wt` subprocess path.
  - [ ] Pin csukuangfj int8 export URLs + sha256 in `catalog_data.rs`; new engine tag wired through `engine/processor.rs`, `config.rs`, `api.rs`, `wt.rs`, `types.ts`.
  - [ ] Extend auto-route: Qwen3 languages (minus Parakeet/GigaAM overlap) → qwen3-asr before Whisper.
  - [ ] CLI smoke runs: zh/ja/ko sample plus an EU-language sample proving Parakeet still wins routing.
- [ ] Phase 3 — Cohere Transcribe evaluation (decision gate, not a commitment)
  - [ ] Run `sherpa-onnx-cohere-transcribe-14-lang` int8 (~2 GB) via CLI on the same samples; compare WER and RTF against whisper-large-v3-turbo-q8.
  - [ ] If clearly better: add as `desktop_only` non-default entry; otherwise record the numbers and close.

## Decisions

- 2026-06-12: Keep Parakeet v3 default, GigaAM v3 for Russian, Whisper as long-tail fallback — research found no successor to any of them.
- 2026-06-12: Phases ordered by risk: runtime bump is low-risk and fixes a shipped crash class; Qwen3-ASR is additive; Cohere is gated on measured benefit because of its 2 GB footprint.

## Verification log

- 2026-06-12: `just check` — 11 jobs green after bumping sherpa-version.txt and sherpa-onnx crates to 1.13.2 (first run failed: offline clippy needed `cargo fetch` for the new crates).
- 2026-06-12: `wt --no-cache --diarizer sortformer-onnx tmp/gpu-test.wav` (sherpa-shared build) — parakeet + sortformer end-to-end on 1.13.2, 1 speaker, 2 segments, transcript JSON written.
- 2026-06-12: `xtask release --dev --no-host --no-deb` — Android prebuilts re-fetched as sherpa-onnx-v1.13.2-android.tar.bz2; dev APK contains all seven native libs with the v1.13.2 sherpa set and a rebuilt libwtranscriber_lib.so.

## Handoff notes
