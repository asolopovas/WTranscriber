# Recording storage and editing

## Durable files

| File                                     | Meaning                                         |
| ---------------------------------------- | ----------------------------------------------- |
| `.meta/<audio filename>.wtmeta.json`     | Trim selection and probed duration              |
| `.meta/<audio filename>.transcript.json` | Transcript, review flags, and edit/undo history |
| `<audio filename>.txt`                   | Adjacent transcript export                      |

These are user data. Clearing the disposable transcript cache preserves durable JSON. Renames move trim metadata, JSON, and text; imports copy trim metadata. Legacy trim sidecars beside recordings remain readable and migrate to `.meta` on save/rename.

Duration probes and trim saves update separate fields under a shared lock. The trim editor saves metadata non-destructively; permanent native trimming is a separate command. Changing target clears the old waveform/range/playhead before loading saved state.

## Waveforms and playback

Folder listing preloads up to 32 waveforms sequentially while transcription is idle, sharing in-flight requests with the editor. Only peaks reach WebView memory. The native cache checks source size/mtime on open and decodes outside its lock.

Fragment playback reads the requested range from a native mono PCM WAV cache and returns standalone WAV via binary IPC. Native decoding publishes complete cache files atomically. The WebView holds at most eight fragments/8 MiB, preloads the first/next segments, and waits for `canplay` before starting at fragment time zero. Source size/mtime changes invalidate both cache and player. Media errors retain decoder codes/messages.

Playback uses saved original-recording timestamps, clamped without padding. Text edits keep their original time range. Timing-sensitive recognition must be checked against audio; a Whisper token-timestamp option alone does not guarantee speech boundaries.

## Transcript edits

Completion, re-diarization, text edits, and speaker changes synchronise JSON and adjacent text under the shared edit lock. Durable JSON is authoritative; a failed disposable-cache update is logged and evicted without failing an otherwise saved edit.

Speaker corrections default to one segment; changing all occurrences is explicit. Word updates match original dialogue order so overlapping times cannot modify neighbours. Segment-only changes preserve other segments and recalculate distinct speaker count. Find/replace is literal and case-sensitive, with count/preview, preserving speakers/timestamps.

Deletion, review marks, recognition retries, text edits, and speaker changes save undo snapshots in durable JSON. The Undo button and Ctrl+Z/Cmd+Z restore them after reopening; editable fields keep native text undo. Failed saves/undo retain history; fresh transcription resets it.

Recognition retries share the native queue and process only the selected segment's original audio range using current ASR settings. They preserve its speaker/timestamps, replace recognised words, and clear its review mark on success. Empty recognition or concurrent transcript changes preserve the old result. Review flags persist and join timing hints in the review filter.

The preview modal is capped at 1000px with viewport-bounded scrolling. Desktop rows offer a trim shortcut; mobile retains the overflow action.

## Removal

File shortcuts ignore editable fields, dialogs, IME composition, and repeated events. Backspace never removes recordings. Bulk removal needs confirmation; queued/running files cannot be removed.

The native command holds the transcription registration lock while moving audio into a unique `.meta/trash/removed-*` folder. Trim/transcript/text files stay in place; restoring the former audio filename recovers them.

Sources: `src-tauri/src/commands/`, `audio_toolkit/`, `transcriber/transcript/`, and the frontend trim/transcript editors. Persistence/playback tests verify these contracts.
