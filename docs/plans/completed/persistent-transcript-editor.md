# Persistent transcript editor

Status: completed
Owner: agent
Started: 2026-10-03
Related docs: [Architecture](../../architecture.md), [ASR pipeline](../../asr-pipeline.md)

## Goal

Preserve trim selections through processing, support segment playback and editing, persist speaker renames, and synchronise the adjacent text transcript.

## Acceptance criteria

- Duration probing cannot overwrite a saved trim, and reopening restores the selection after transcription.
- Completed segments play the original audio at their absolute timestamps and stop at the segment end.
- Text editing and a speaker rename modal persist changes and report save failures without discarding drafts.
- Completed results persist in `.meta` and produce an adjacent `.txt`, updated after edits and speaker renames.
- Repetition cleanup covers tokenless segment loops without removing isolated rhetorical repetition.

## Current context

Implemented in the existing checkout and branch. User recordings were inspected for diagnosis but were not rewritten.

## Steps

- [x] Protect trim metadata updates and saving during editor preparation.
- [x] Add durable transcript persistence, text synchronisation, and editing commands.
- [x] Build playback, segment editing, and speaker rename UI.
- [x] Cover repetition loops and persistence with regression tests.
- [x] Run native, frontend, and browser checks; document behaviour.

## Decisions

- 2026-10-03: Graphify locates relevant paths; current source confirms them. Preserve original recording timestamps for playback. Keep user metadata in `.meta`, outside disposable caches.

## Verification log

- 2026-10-03: Native suite passed serially: 239 passed, 2 model-dependent tests ignored. A parallel run hit the existing shared-log tail assertion; the serial run passed.
- 2026-10-03: Rust Clippy passed with warnings denied.
- 2026-10-03: Frontend tests passed: 127 tests. Typecheck and Vue lint passed.
- 2026-10-03: Five focused Playwright flows passed with mocked native IPC; desktop/mobile screenshots inspected.
- 2026-10-03: Native tests cover actual JSON/TXT writes, cache removal, trim preservation, rename rollback, and the reported repetition shape.
- 2026-10-03: The UI detector reported no findings.

## Handoff notes

- Logs showed the old scissors action invoking permanent audio trimming and clearing trim metadata. The normal trim editor now only saves metadata. Existing physically shortened recordings are not restored automatically.
- Probe/save read-modify-write races could also discard a newer range; field-specific updates now share a lock.
- The reported phrase appeared as repeated whole-phrase Whisper tokens, including a short intervening phrase. Cleanup now covers this representation and tokenless loops. Existing saved transcripts remain intact until explicitly re-transcribed or edited.
- Changes are implemented and verified in source; the installed desktop application requires a rebuild/restart.
