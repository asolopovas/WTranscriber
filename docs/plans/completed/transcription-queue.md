# Reliable transcription queue and dependency refresh

Status: completed
Owner: agent
Started: 2026-10-03
Related docs: [ASR pipeline](../../asr-pipeline.md), [verification](../../verification.md)

## Goal

Keep transcription reliable when more files are added during active work, isolate cancellation and failure, and update dependencies with verified compatibility.

## Acceptance criteria

- Manual, batch and re-diarization requests use one additive FIFO queue.
- Native work stays serial until the current worker actually finishes.
- Duplicate paths, individual errors and cancellations cannot stop unrelated jobs.
- Regression tests cover queue transitions and the reported UI sequence.
- Dependency upgrades preserve supported decoding and inference behaviour; incompatible latest releases have explicit reasons.

## Current context

Work is on the existing `main` branch in `/home/andrius/src/WTranscriber`. Installation and release-workflow edits are concurrent user work and must be preserved. Graphify located lifecycle code in WTranscriber and the comparable Handy checkout (`Handly` does not exist).

## Steps

- [x] Trace frontend and native cancellation ownership with Graphify and source inspection.
- [x] Implement shared frontend queue and native completion-owned execution slot.
- [x] Add deterministic queue unit tests and UI regression tests.
- [x] Audit and upgrade dependency versions; adapt incompatible APIs.
- [x] Finish native compilation, decoder and inference verification on upgraded dependencies.
- [x] Run final relevant checks and document remaining platform limits.

## Decisions

- 2026-10-03: Keep one native inference job active because engine state and worker resources are shared.
- 2026-10-03: Follow Handy's single-owner transitions and completion guard, without copying its global cancellation semantics into a multi-file application.
- 2026-10-03: Retain TypeScript 6 because TypeScript 7 cannot run the latest Vue type checker; the failure was reproduced.
- 2026-10-03: Replace manual per-format sample conversion with Symphonia's checked conversion API while migrating to 0.6; test PCM and valid non-MP4 SL descriptors.

- 2026-10-03: Pin Parakeet 0.3.7 to retain installed four-speaker models and Sherpa wrapper/sys 1.13.3 to match the DirectML-compatible ONNX runtime. Keep ONNX API 24 explicit and link one runtime through Sherpa.
- 2026-10-03: Match Linux Whisper C++ ABI to the prebuilt Sherpa runtime. A static-build ONNX initialisation crash was reproduced, traced with GDB, and eliminated by rebuilding Whisper with the target-specific ABI flag.

## Verification log

- 2026-10-03: Initial backend command tests passed (18), including seven queue regressions, before dependency upgrades.
- 2026-10-03: Frontend queue tests passed (13); full JS suite passed (109); typecheck, Vue lint, Knip and frontend production build passed.
- 2026-10-03: All eleven browser scenarios passed, including appending remaining files, configuration snapshots and cancellation/failure isolation. Older fixture assumptions were corrected to current IPC and UI.

- 2026-10-03: Final native library suites passed 216 tests each with `sherpa-shared` and `sherpa-static`. Both installed-model tests (Silero recurrent inference and four-speaker Sortformer) passed in each mode. Strict all-targets shared Clippy and Rust formatting passed.
- 2026-10-03: Build-tool tests passed (5), build-tool Clippy and the standalone worker check passed. Documentation lint, dependency checks, and Cargo/Bun audits passed under the existing audit policy.
- 2026-10-03: Windows, Android, and CUDA runtime/build verification was not performed. Linux static and shared inference was verified.
- 2026-10-03: AAC and Opus fixtures contain generated 440 Hz sine waves (0.1 seconds), encoded with FFmpeg. Tests use the checked-in bytes and do not require FFmpeg.

## Handoff notes

No matching incident trace establishes why the original start-remaining action first failed. Confirmed defects are row-level cancel-all, duplicate cancellation-token replacement, separate queue ownership and release of the native lock before cancellation finishes. Do not claim an unobserved resource-exhaustion event.
