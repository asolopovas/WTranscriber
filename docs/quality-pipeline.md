# WhisperX alignment and Community-1

Desktop users can choose **WhisperX + Community-1** in the diarizer selector. The selected native ASR model still transcribes the audio, including whisper.cpp large-v3-turbo. No second Whisper ASR model is loaded. WhisperX aligns the recognised words, then pyannote Community-1 detects speakers over the selected trim for consistent voice identity. Re-diarize preserves the existing text and runs the same alignment and speaker stages.

## Install once

Install Python 3.11 with `uv`, then create an isolated environment. On Linux:

```bash
uv venv --python 3.11 ~/.local/share/wtranscriber/quality/.venv
uv pip install --python ~/.local/share/wtranscriber/quality/.venv/bin/python -r src-tauri/python/requirements.txt
~/.local/share/wtranscriber/quality/.venv/bin/hf auth login
```

Accept the [Community-1 conditions](https://huggingface.co/pyannote/speaker-diarization-community-1) and authenticate with a read token that can access approved gated models. Tokens stay in Hugging Face's credential store, outside WTranscriber's configuration and command arguments. Model downloads happen on first use. Audio is processed locally; telemetry is disabled in the worker.

The default interpreter is `quality/.venv/bin/python` under the application's data directory, or `quality/.venv/Scripts/python.exe` on Windows. `WT_QUALITY_PYTHON` can select an absolute interpreter path for other installations. The worker script is embedded in the Rust binary, so installation does not depend on the source checkout.

## Timing and speaker behaviour

- Decode once to the existing cached mono 16 kHz WAV; load that audio in memory for alignment and diarization.
- Native ASR receives the saved trim. Alignment uses that same range and adds the trim offset back exactly once. Diarization uses the same complete trim, so unrelated voices outside it do not affect speaker clustering.
- English uses the larger `WAV2VEC2_ASR_LARGE_LV60K_960H` alignment model; other languages use WhisperX's language-specific defaults.
- Align nearby sentences together in bounded windows, with up to one second of analysis context inside the trim. This is not playback padding: output segments start and end at the aligned words.
- Assign each aligned word by greatest temporal overlap with Community-1's exclusive timeline, then split at speaker changes, sentence ends and long gaps. Do not smooth away short replies or guess speakers for words without overlap.
- Preserve words that cannot be aligned, leave their speaker unassigned and report a warning. Their fallback timing is approximate. Forced alignment cannot recover words absent from the ASR text or guarantee correct identities in overlapping speech.
- Respect a known speaker count; otherwise use automatic detection. Two speakers in an excerpt do not imply only two speakers in the whole recording.
- Retain the selected ASR model and release its worker before loading Python models. The alignment model is released before diarization to reduce GPU memory pressure.

The previous JSON and text export are backed up under the source's `.meta/backups/before-quality-*` directory before successful replacement. Saved trims are never rewritten. Cancellation or model failure leaves the existing transcript intact. The cache key includes the diarizer and quality pipeline version, so switching backends cannot reuse a result from another diarizer.

## Verification

```bash
python3 -m unittest discover -s src-tauri/python -p 'test_*.py'
```

Native tests cover trim validation, cache invalidation and persistence; UI tests cover desktop-only selection. Actual model quality must also be checked against known speaker changes and word boundaries in the source recording. Do not claim perfect diarization from a passing synthetic test.

Sources: [WhisperX](https://github.com/m-bain/whisperX), [Community-1 model card](https://huggingface.co/pyannote/speaker-diarization-community-1).
