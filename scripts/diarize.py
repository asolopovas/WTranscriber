import argparse
import io
import json
import os
import sys
import tempfile

os.environ["HF_HUB_DISABLE_SYMLINKS_WARNING"] = "1"
os.environ["HF_HUB_DISABLE_SYMLINKS"] = "1"
os.environ.setdefault("PYTORCH_CUDA_ALLOC_CONF", "expandable_segments:True")

if sys.platform == "win32":
    _OrigTempDir = tempfile.TemporaryDirectory

    class _WinTempDir(_OrigTempDir):
        def __init__(self, *args, **kwargs):
            kwargs.setdefault("ignore_cleanup_errors", True)
            super().__init__(*args, **kwargs)

        def cleanup(self):
            try:
                super().cleanup()
            except (OSError, PermissionError):
                pass

        def __exit__(self, exc, value, tb):
            try:
                super().__exit__(exc, value, tb)
            except (OSError, PermissionError):
                pass

    tempfile.TemporaryDirectory = _WinTempDir


def _resolve_symlink(fpath: str) -> str:
    try:
        target = os.readlink(fpath)
    except OSError:
        try:
            return os.path.realpath(fpath)
        except OSError:
            return ""
    if not os.path.isabs(target):
        target = os.path.normpath(os.path.join(os.path.dirname(fpath), target))
    return target


def _fix_hf_symlinks(model_id: str) -> None:
    cache_dir = os.path.join(os.path.expanduser("~"), ".cache", "huggingface", "hub")
    model_dir = os.path.join(cache_dir, "models--" + model_id.replace("/", "--"))
    snapshots = os.path.join(model_dir, "snapshots")
    blobs = os.path.join(model_dir, "blobs")
    if not os.path.isdir(snapshots) or not os.path.isdir(blobs):
        return
    import shutil

    for root, _dirs, files in os.walk(snapshots):
        for fname in files:
            fpath = os.path.join(root, fname)
            try:
                is_link = os.path.islink(fpath)
            except OSError:
                is_link = False
            if not is_link:
                continue
            try:
                exists = os.path.exists(fpath)
            except OSError:
                exists = False
            target = _resolve_symlink(fpath)
            if not target:
                continue
            try:
                target_ok = os.path.isfile(target)
            except OSError:
                target_ok = False
            if exists and target_ok:
                continue
            if not target_ok:
                continue
            try:
                os.remove(fpath)
                shutil.copy2(target, fpath)
                print(f"fixed broken symlink: {fname}", file=sys.stderr, flush=True)
            except OSError as e:
                print(f"warn: could not fix symlink {fname}: {e}", file=sys.stderr, flush=True)


def main():
    parser = argparse.ArgumentParser(description="Speaker diarization (NeMo Sortformer)")
    parser.add_argument("wav", help="Path to WAV file (16kHz mono)")
    parser.add_argument("--num-speakers", type=int, default=0)
    args = parser.parse_args()

    real_stdout = os.dup(1)
    os.dup2(2, 1)
    sys.stdout = io.TextIOWrapper(os.fdopen(os.dup(2), "wb"), line_buffering=True)

    model_id = "nvidia/diar_streaming_sortformer_4spk-v2"
    _fix_hf_symlinks(model_id)

    try:
        from nemo.collections.asr.models import SortformerEncLabelModel
    except ImportError:
        print("nemo_toolkit not installed. Install with: uv pip install 'nemo_toolkit[asr]'", file=sys.stderr)
        sys.exit(1)

    import torch

    force_cpu = os.environ.get("WT_DIAR_DEVICE") == "cpu"
    device = "cpu" if force_cpu else ("cuda" if torch.cuda.is_available() else "cpu")
    print(f"device: {device}", file=sys.stderr, flush=True)
    print("loading model...", file=sys.stderr, flush=True)

    def _configure_streaming(m):
        m.sortformer_modules.chunk_len = 340
        m.sortformer_modules.chunk_right_context = 40
        m.sortformer_modules.fifo_len = 40
        m.sortformer_modules.spkcache_update_period = 300
        m.sortformer_modules.spkcache_len = 188
        m.sortformer_modules.log = False
        m.sortformer_modules._check_streaming_parameters()

    def _load(dev):
        m = SortformerEncLabelModel.from_pretrained(model_id, map_location=dev)
        m.eval()
        _configure_streaming(m)
        return m

    model = _load(device)
    print("processing...", file=sys.stderr, flush=True)

    if args.num_speakers > 0:
        print(
            f"note: SortformerEncLabelModel is fixed at 4 speakers; ignoring --num-speakers={args.num_speakers}",
            file=sys.stderr,
            flush=True,
        )

    try:
        result = model.diarize(audio=args.wav, batch_size=1)
    except torch.OutOfMemoryError as e:
        if device != "cpu":
            print(f"CUDA OOM ({e}); retrying on CPU...", file=sys.stderr, flush=True)
            del model
            torch.cuda.empty_cache()
            device = "cpu"
            model = _load(device)
            result = model.diarize(audio=args.wav, batch_size=1)
        else:
            raise

    os.dup2(real_stdout, 1)
    sys.stdout = io.TextIOWrapper(os.fdopen(1, "wb"), line_buffering=True)

    output = []
    speakers = set()
    for seg_list in result:
        for seg in seg_list:
            parts = seg.split()
            if len(parts) == 3:
                start, end, spk = float(parts[0]), float(parts[1]), parts[2]
                spk_clean = spk.replace("speaker_", "")
                try:
                    label = f"SPEAKER_{int(spk_clean):02d}"
                except ValueError:
                    label = spk.upper()
                speakers.add(label)
                output.append({"start": round(start, 3), "end": round(end, 3), "speaker": label})

    output.sort(key=lambda s: s["start"])
    print(f"done: {len(speakers)} speakers, {len(output)} segments", file=sys.stderr, flush=True)
    json.dump(output, sys.stdout, indent=None)
    sys.stdout.write("\n")
    sys.stdout.flush()


if __name__ == "__main__":
    main()
