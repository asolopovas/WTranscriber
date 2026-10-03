# Fragment timing correction

Status: completed
Owner: agent
Started: 2026-10-03
Related docs: [ASR pipeline](../../asr-pipeline.md), [Architecture](../../architecture.md)

## Goal

Make fragment playback useful for older approximate timestamps and regenerate the reported recording with verified word-level boundaries.

## Acceptance criteria

- Saved playback context adds one second before and three seconds after, within track bounds; exact playback remains available.
- Parakeet/Sherpa uses measured token durations instead of extending words through trailing silence.
- Final transcript ranges remain within recording duration.
- Preserve a backup before replacing the user's transcript; verify the reported phrases and adjacent text export.

## Current context

The reported clips contained leading noise because Whisper's intervals preceded speech. Its token-timestamp option still allocated silence to a word in a diagnostic excerpt. A separate Parakeet run located speech later. Source review also found measured token durations were ignored by word coalescing.

## Steps

- [x] Compare reported excerpts with local recognition.
- [x] Add a saved playback-context preference and browser coverage.
- [x] Consume token durations and clamp transcript boundaries.
- [x] Back up existing transcript, text export, trim metadata, and cached result.
- [x] Finish the corrected regeneration and verify output.

## Decisions

- 2026-10-03: Use the installed Parakeet model after Whisper precise-timing checks remained inaccurate; use shorter recognition chunks for the replacement. Keep source audio unchanged.
- 2026-10-03: Playback context does not alter text or saved timestamps. CLI JSON exports follow the existing `.meta` storage convention.

## Verification log

- Typecheck, Vue lint, 137 JavaScript tests, and two playback browser tests passed.
- Rust Clippy passed; 244 native tests passed serially, with three external-fixture/model checks ignored.
- Token-duration regressions cover trailing silence, subword grouping, and recording-end bounds.

## Handoff notes

The replacement contains 580 segments, 5,258 word entries, and four speaker labels. Every segment lies within recording bounds. Source-audio SHA-256 and trim-metadata bytes match the pre-run backup, and the adjacent TXT contains the regenerated text. The reported examples now start at 176.778 s and 345.859 s; short extracted clips and independent local recognition were used for spot checks. Word timings and transcript wording remain model estimates rather than a guarantee of word-perfect transcription.

Explicit CLI `--no-cache` now bypasses both the disposable cache and durable saved transcripts, clearing partial work for the actual trim-dependent key. A scratch-recording run verifies that a deliberately marked saved result is replaced by fresh recognition. The original backup is retained locally beside the recording under `.meta/backups`.
