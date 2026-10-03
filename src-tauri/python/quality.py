import contextlib
import gc
import json
import math
import os
from pathlib import Path
import sys

os.environ.setdefault("PYANNOTE_METRICS_ENABLED", "0")
os.environ.setdefault("HF_HUB_DISABLE_TELEMETRY", "1")
PROTOCOL = sys.stdout


def progress(phase, percent):
    PROTOCOL.write(json.dumps({"phase": phase, "percent": percent}) + "\n")
    PROTOCOL.flush()


def speaker_for(start, end, turns):
    overlaps = {}
    for a, b, speaker in turns:
        overlap = min(end, b) - max(start, a)
        if overlap > 0:
            overlaps[speaker] = overlaps.get(speaker, 0) + overlap
    return max(overlaps, key=overlaps.get) if overlaps else None


def make_words(segments, turns, offset, limit):
    result = []
    unaligned = 0
    for segment in segments:
        words = segment.get("words", [])
        if not words:
            words = [{"word": segment["text"]}]
        for word in words:
            text = word["word"].strip()
            if not text:
                continue
            measured = all(k in word and math.isfinite(word[k]) for k in ("start", "end"))
            start = word["start"] if measured else segment["start"]
            end = word["end"] if measured else segment["end"]
            if not measured:
                unaligned += 1
            start = min(limit, max(offset, round(start * 1000) + offset))
            end = min(limit, max(start, round(end * 1000) + offset))
            result.append({"text": text, "start_ms": start, "end_ms": end,
                           "speaker": speaker_for(start / 1000, end / 1000, turns) if measured else None,
                           "confidence": float(word.get("score", 0)) if measured else 0.0})
    return result, unaligned


def alignment_windows(utterances, start, end):
    windows = []
    for u in utterances:
        if not u["text"].strip() or u["end_ms"] <= start or u["start_ms"] >= end:
            continue
        a = max(0, (u["start_ms"] - start) / 1000)
        b = min((end - start) / 1000, (u["end_ms"] - start) / 1000)
        if windows and b - windows[-1]["start"] <= 30 and a - windows[-1]["end"] <= 2:
            windows[-1]["text"] += " " + u["text"]
            windows[-1]["end"] = max(b, windows[-1]["end"])
        else:
            windows.append({"start": a, "end": b, "text": u["text"]})
    for window in windows:
        window["start"] = max(0, window["start"] - 1)
        window["end"] = min((end - start) / 1000, window["end"] + 1)
    return windows


def release(torch):
    gc.collect()
    if torch.cuda.is_available():
        torch.cuda.empty_cache()


def run(request):
    import numpy as np
    import torch
    import whisperx
    from huggingface_hub import get_token
    from pyannote.audio import Pipeline

    torch.set_num_threads(max(1, request.get("threads", 4)))
    device = request["device"]
    if device == "cuda" and not torch.cuda.is_available():
        raise RuntimeError("CUDA is unavailable to the quality worker. Choose CPU or install CUDA-enabled PyTorch.")
    token = get_token()
    if not token:
        raise RuntimeError("Community-1 needs Hugging Face access. Accept its model conditions, then run hf auth login in the quality environment.")
    progress("loading_audio", 0)
    audio = whisperx.load_audio(request["wav"])
    duration = round(len(audio) / 16)
    start = min(duration, request.get("start_ms", 0))
    end = min(duration, request.get("end_ms") or duration)
    if end <= start:
        raise ValueError("The selected trim contains no audio.")
    selected = np.ascontiguousarray(audio[start * 16:end * 16])
    language = request["language"]
    if language == "auto" or not language:
        language = None
    previous = request["previous"]
    progress("transcribing", 0)
    language = previous["language"]
    segments = alignment_windows(previous["utterances"], start, end)
    model_name = previous["model"]
    progress("transcribing", 60)
    original_text = "".join("".join(s["text"].split()) for s in segments)
    if segments:
        aligner, metadata = whisperx.load_align_model(language_code=language, device=device,
                                                      model_name="WAV2VEC2_ASR_LARGE_LV60K_960H" if language == "en" else None)
        aligned = whisperx.align(segments, aligner, metadata, selected, device,
                                 interpolate_method="ignore", return_char_alignments=False)
        del aligner
        release(torch)
        segments = aligned["segments"]
    progress("transcribing", 100)
    progress("diarizing", 0)
    pipeline = Pipeline.from_pretrained("pyannote/speaker-diarization-community-1", token=token)
    pipeline.to(torch.device(device))
    last = [0.0]

    def hook(step_name, step_artifact, file=None, total=None, completed=None):
        if total and completed is not None and step_name in ("segmentation", "embeddings"):
            base = 0 if step_name == "segmentation" else 50
            pct = base + 49 * completed / total
            if pct > last[0]:
                last[0] = pct
                progress("diarizing", pct)

    output = pipeline({"waveform": torch.from_numpy(selected[None, :]), "sample_rate": 16000},
                      num_speakers=request.get("speakers") or None, hook=hook)
    turns = [(turn.start + start / 1000, turn.end + start / 1000, speaker) for turn, _, speaker in
             output.exclusive_speaker_diarization.itertracks(yield_label=True)]
    words, unaligned = make_words(segments, turns, start, end)
    aligned_text = "".join("".join(w["text"].split()) for w in words)
    if aligned_text != original_text:
        raise RuntimeError("Alignment changed or omitted transcript text. The existing transcript has been kept.")
    warnings = []
    if unaligned:
        warnings.append(f"{unaligned} words could not be aligned: their timing remains approximate and their speaker is unassigned.")
    uncertain = sum(w["confidence"] < 0.3 for w in words)
    if uncertain:
        warnings.append(f"{uncertain} words have low alignment confidence. Review their timing against the recording.")
    progress("diarizing", 100)
    return {"words": words, "language": language or "auto", "duration_ms": duration,
            "model": model_name, "warnings": warnings}


def main():
    request = json.loads(Path(sys.argv[1]).read_text())
    with contextlib.redirect_stdout(sys.stderr):
        result = run(request)
    Path(sys.argv[2]).write_text(json.dumps(result, ensure_ascii=False, allow_nan=False))


if __name__ == "__main__":
    main()
