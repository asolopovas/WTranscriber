import { afterEach, describe, expect, it, vi } from "vitest";
import { createAudioSegmentCache, waitForPlayableAudio } from "./audioSegmentCache";

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("audio fragment cache", () => {
  it("does not repopulate the cache from an invalidated request", async () => {
    let resolve!: (bytes: ArrayBuffer) => void;
    const read = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<ArrayBuffer>((done) => {
            resolve = done;
          }),
      )
      .mockResolvedValue(new ArrayBuffer(10));
    const cache = createAudioSegmentCache(read, 20);
    const pending = cache.load(0, 1);
    cache.clear();
    resolve(new ArrayBuffer(10));
    await pending;
    await cache.load(0, 1);
    expect(read).toHaveBeenCalledTimes(2);
  });
  it("coalesces preparation and reuses WAV blobs", async () => {
    const read = vi.fn().mockResolvedValue(new ArrayBuffer(10));
    const cache = createAudioSegmentCache(read);
    const first = cache.load(1000, 2000);
    const second = cache.load(1000, 2000);
    expect(first).toBe(second);
    const blob = await first;
    expect(blob.type).toBe("audio/wav");
    expect(await cache.load(1000, 2000)).toBe(blob);
    expect(read).toHaveBeenCalledTimes(1);
  });
  it("evicts least recently used fragments within the memory budget", async () => {
    const read = vi.fn().mockResolvedValue(new ArrayBuffer(10));
    const cache = createAudioSegmentCache(read, 20);
    await cache.load(0, 1);
    await cache.load(1, 2);
    await cache.load(0, 1);
    await cache.load(2, 3);
    await cache.load(0, 1);
    expect(read).toHaveBeenCalledTimes(3);
    await cache.load(1, 2);
    expect(read).toHaveBeenCalledTimes(4);
  });
  it("does not cache failed, oversized or invalidated preparation", async () => {
    const read = vi
      .fn()
      .mockRejectedValueOnce(new Error("missing source"))
      .mockResolvedValue(new ArrayBuffer(100));
    const cache = createAudioSegmentCache(read, 20);
    await expect(cache.load(0, 1)).rejects.toThrow("missing source");
    await cache.load(0, 1);
    await cache.load(0, 1);
    expect(read).toHaveBeenCalledTimes(3);
    const pending = cache.load(2, 3);
    cache.clear();
    await pending;
    await cache.load(2, 3);
    expect(read).toHaveBeenCalledTimes(5);
  });
});

describe("audio readiness", () => {
  it("waits until the fragment can play", async () => {
    const audio = document.createElement("audio");
    let resolved = false;
    const pending = waitForPlayableAudio(audio, new AbortController().signal).then(() => {
      resolved = true;
    });
    await Promise.resolve();
    expect(resolved).toBe(false);
    audio.dispatchEvent(new Event("loadedmetadata"));
    await Promise.resolve();
    expect(resolved).toBe(false);
    audio.dispatchEvent(new Event("canplay"));
    await pending;
    expect(resolved).toBe(true);
  });
  it("cancels pending playback when another segment is selected", async () => {
    const audio = document.createElement("audio");
    const controller = new AbortController();
    const pending = waitForPlayableAudio(audio, controller.signal);
    controller.abort();
    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
  });
  it("reports decoder details and times out stalled loading", async () => {
    const audio = document.createElement("audio");
    const pending = waitForPlayableAudio(audio, new AbortController().signal);
    Object.defineProperty(audio, "error", { value: { code: 3, message: "decode failed" } });
    audio.dispatchEvent(new Event("error"));
    await expect(pending).rejects.toThrow("media error 3");
    vi.useFakeTimers();
    const stalled = waitForPlayableAudio(
      document.createElement("audio"),
      new AbortController().signal,
    );
    const assertion = expect(stalled).rejects.toThrow("too long");
    await vi.advanceTimersByTimeAsync(15000);
    await assertion;
  });
});
