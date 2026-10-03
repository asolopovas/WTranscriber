# CUDA runtime selection and browser verification

Status: completed
Owner: agent
Started: 2026-10-03
Related docs: [ASR pipeline](../../asr-pipeline.md), [dev loop](../../dev-loop.md), [release](../../release.md)

## Goal

Fix the reported CPU-fallback/missing-Sherpa failure, enable supported GPU inference on this host, and test application flows in a development browser.

## Acceptance criteria

- Unsupported CUDA requests resolve to CPU before execution-path selection.
- Linux dev/install selects and packages usable CUDA support when NVIDIA hardware and a compatible toolkit are present.
- Native GPU inference is verified on the installed RTX 3070, with CPU fallback retained.
- Browser testing covers the main file, queue, configuration, transcript and maintenance workflows.

## Current context

The host has an RTX 3070 with 8 GiB VRAM, driver 580.178.04 and CUDA toolkit 12.0. Local CUDA compilation requires GCC 12 rather than the default GCC 13. Existing installed models permit real inference without downloading model weights.

## Steps

- [x] Reproduce the fallback bug with a failing native regression test.
- [x] Correct the resolved device and verify all three ONNX engine dispatch paths.
- [x] Start Vite in the in-app browser with explicitly labelled simulated Tauri commands.
- [x] Expand browser E2E coverage and fix menu target loss discovered by those tests.
- [x] Complete CUDA dev/install packaging and GPU inference checks.
- [x] Run final checks and record precise limits.

## Decisions

- 2026-10-03: The fallback warning previously left `Device::Cuda` unchanged. Dispatch then selected an external executable and failed even though the CPU recognizer was bundled. The job now uses the resolved device.
- 2026-10-03: Keep browser mock verification separate from real native/GPU inference evidence; the ordinary browser cannot host Tauri's native IPC bridge.
- 2026-10-03: Small VAD/language probes remain on CPU; GPU selection applies to heavy inference where supported. Diarizers honour the requested device and retain CPU fallback.

## Verification log

- 2026-10-03: Fallback regression failed before the fix (`Cuda` instead of `Cpu`) and passed afterwards for all three ONNX engines.
- 2026-10-03: CPU native suite passed 218 tests before the additional diarizer routing checks; strict Clippy passed.
- 2026-10-03: All 23 browser E2E tests and 115 JavaScript tests passed; typecheck, E2E TypeScript checking, Vue lint and Knip passed. Tests include synthetic recording, trim save/reset, speaker rename/copy, and missing-model recovery. Manual in-app-browser checks confirmed additive queueing, queued cancellation and failure isolation.
- 2026-10-03: CUDA CLI built successfully with GCC 12 and architecture 86. Actual CUDA inference passed for Parakeet, GigaAM, Qwen3-ASR, Whisper, Sortformer and TitaNet. NVIDIA process memory and runtime-provider logs confirmed GPU use.

- 2026-10-03: CUDA-feature native suite passed 220 tests with two model-dependent tests ignored. Installer/backend-selection tests passed (14). Real inference also worked without a custom shell library path once GPU libraries were beside the executable.

- 2026-10-03: Strict all-targets CUDA Clippy passed. Actual `just install` built and activated the GPU release using GCC 12 and architecture 86; installed GUI hash matched the updated release binary. Installed Whisper and GigaAM CUDA transcription succeeded with `LD_LIBRARY_PATH` unset. Active install: `/home/andrius/.local/lib/wtranscriber/build-6eubMW`.
- 2026-10-03: GPU evidence and generated-speech transcripts are in `tmp/cuda-runtime-staging/`. Vite browser preview remains at `http://localhost:1420/`, served by the task-owned `tmp/browser-e2e-preview.ts`.

## Handoff notes

The browser preview uses the shared E2E fixture and visibly identifies simulated native commands. It must not be reported as proof of native transcription or CUDA. Preserve the distinction in final reporting.

Restart any already-running WTranscriber process to load the newly installed executable. No actual microphone was captured during browser testing; platform-specific Windows/Android execution was not tested in this follow-up.
