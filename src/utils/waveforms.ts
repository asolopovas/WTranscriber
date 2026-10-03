import { api } from "@/api";
import type { DirEntry } from "@/types";

type Source = Pick<DirEntry, "path" | "size_bytes" | "modified_ms">;
const MAX_PRELOAD = 32;
const sourceKey = (source: Source) =>
  JSON.stringify([source.path, source.size_bytes, source.modified_ms]);

export function createWaveformLoader(read: (path: string) => Promise<number[]>) {
  const pending = new Map<string, Promise<number[]>>();
  const warmed = new Set<string>();
  let queue: Source[] = [];
  let running = false;

  function load(source: Source): Promise<number[]> {
    const key = sourceKey(source);
    const existing = pending.get(key);
    if (existing) return existing;
    const request = Promise.resolve()
      .then(() => read(source.path))
      .then((peaks) => {
        warmed.delete(key);
        warmed.add(key);
        if (warmed.size > MAX_PRELOAD) warmed.delete(warmed.values().next().value!);
        return peaks;
      })
      .finally(() => pending.delete(key));
    pending.set(key, request);
    return request;
  }

  async function drain() {
    if (running) return;
    running = true;
    try {
      while (queue.length) {
        const source = queue.shift()!;
        if (warmed.has(sourceKey(source))) continue;
        try {
          await load(source);
        } catch {}
      }
    } finally {
      running = false;
    }
  }

  function preload(sources: Source[]) {
    queue = sources.slice(0, MAX_PRELOAD).map((source) => ({ ...source }));
    void drain();
  }

  return { load, preload };
}

export const waveforms = createWaveformLoader((path) => api.audioWaveform(path, 320));
