export function segmentPlaybackRange(
  segment: { start_ms: number; end_ms: number },
  durationMs: number,
) {
  const end = Math.floor(Math.max(0, Math.min(segment.end_ms, durationMs)));
  const start = Math.floor(Math.max(0, Math.min(segment.start_ms, end)));
  return { start, end };
}

export function createAudioSegmentCache(
  read: (startMs: number, endMs: number) => Promise<ArrayBuffer>,
  maxBytes = 8 * 1024 * 1024,
) {
  const entries = new Map<string, Blob>();
  const pending = new Map<string, Promise<Blob>>();
  let usedBytes = 0;
  let generation = 0;

  function load(startMs: number, endMs: number): Promise<Blob> {
    const key = `${startMs}:${endMs}`;
    const cached = entries.get(key);
    if (cached) {
      entries.delete(key);
      entries.set(key, cached);
      return Promise.resolve(cached);
    }
    const existing = pending.get(key);
    if (existing) return existing;
    const current = generation;
    const request = read(startMs, endMs)
      .then((bytes) => {
        const blob = new Blob([bytes], { type: "audio/wav" });
        if (current === generation && blob.size <= maxBytes) {
          while (entries.size && (usedBytes + blob.size > maxBytes || entries.size >= 8)) {
            const oldest = entries.keys().next().value!;
            usedBytes -= entries.get(oldest)!.size;
            entries.delete(oldest);
          }
          entries.set(key, blob);
          usedBytes += blob.size;
        }
        return blob;
      })
      .finally(() => {
        if (pending.get(key) === request) pending.delete(key);
      });
    pending.set(key, request);
    return request;
  }

  function clear() {
    generation += 1;
    entries.clear();
    pending.clear();
    usedBytes = 0;
  }

  return { load, clear };
}

export function mediaErrorMessage(audio: HTMLMediaElement): string {
  const error = audio.error;
  const detail = error?.message ? `: ${error.message}` : "";
  return `Audio playback failed (media error ${error?.code ?? "unknown"})${detail}`;
}

export function waitForPlayableAudio(audio: HTMLMediaElement, signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.reject(new DOMException("Playback cancelled", "AbortError"));
  if (audio.error) return Promise.reject(new Error(mediaErrorMessage(audio)));
  if (audio.readyState >= 2) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const finish = (error?: Error) => {
      clearTimeout(timeout);
      audio.removeEventListener("canplay", ready);
      audio.removeEventListener("error", failed);
      signal.removeEventListener("abort", aborted);
      if (error) reject(error);
      else resolve();
    };
    const ready = () => finish();
    const failed = () => finish(new Error(mediaErrorMessage(audio)));
    const aborted = () => finish(new DOMException("Playback cancelled", "AbortError"));
    const timeout = setTimeout(
      () =>
        finish(new Error("Audio took too long to become ready. Try playing the segment again.")),
      15000,
    );
    audio.addEventListener("canplay", ready, { once: true });
    audio.addEventListener("error", failed, { once: true });
    signal.addEventListener("abort", aborted, { once: true });
  });
}
