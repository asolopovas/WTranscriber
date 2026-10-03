import { flushPromises } from "@vue/test-utils";
import { describe, expect, it, vi } from "vitest";
import { createWaveformLoader } from "./waveforms";

const source = (path: string, modified_ms = 1, size_bytes = 10) => ({
  path,
  modified_ms,
  size_bytes,
});

describe("background waveforms", () => {
  it("loads sequentially and shares an in-flight editor request", async () => {
    let finish!: (peaks: number[]) => void;
    const read = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<number[]>((resolve) => {
            finish = resolve;
          }),
      )
      .mockResolvedValue([0.5]);
    const loader = createWaveformLoader(read);
    loader.preload([source("a"), source("b")]);
    await flushPromises();
    const foreground = loader.load(source("a"));
    expect(read.mock.calls).toEqual([["a"]]);
    finish([0.2]);
    expect(await foreground).toEqual([0.2]);
    await flushPromises();
    expect(read.mock.calls).toEqual([["a"], ["b"]]);
    loader.preload([source("a"), source("b")]);
    await flushPromises();
    expect(read).toHaveBeenCalledTimes(2);
    await loader.load(source("a"));
    expect(read).toHaveBeenCalledTimes(3);
  });

  it("drops queued work when paused or switching folders", async () => {
    let finish!: (peaks: number[]) => void;
    const read = vi
      .fn()
      .mockImplementationOnce(
        () =>
          new Promise<number[]>((resolve) => {
            finish = resolve;
          }),
      )
      .mockResolvedValue([]);
    const loader = createWaveformLoader(read);
    loader.preload([source("a"), source("b")]);
    await flushPromises();
    loader.preload([]);
    finish([]);
    await flushPromises();
    expect(read.mock.calls).toEqual([["a"]]);
    loader.preload([source("c")]);
    await flushPromises();
    expect(read.mock.calls).toEqual([["a"], ["c"]]);
  });

  it("retries failures and preloads changed sources", async () => {
    const read = vi.fn().mockRejectedValueOnce(new Error("unavailable")).mockResolvedValue([]);
    const loader = createWaveformLoader(read);
    loader.preload([source("a"), source("b")]);
    await flushPromises();
    await loader.load(source("a"));
    loader.preload([source("a", 2), source("b", 1, 20)]);
    await flushPromises();
    expect(read.mock.calls).toEqual([["a"], ["b"], ["a"], ["a"], ["b"]]);
  });

  it("bounds preloading to the native cache capacity", async () => {
    const read = vi.fn().mockResolvedValue([]);
    const loader = createWaveformLoader(read);
    loader.preload(Array.from({ length: 100 }, (_, i) => source(String(i))));
    await flushPromises();
    expect(read).toHaveBeenCalledTimes(32);
  });
});
