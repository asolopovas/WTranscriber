import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h, nextTick, ref, type Ref } from "vue";
import { mount } from "@vue/test-utils";
import { useDebouncedSave } from "./useDebouncedSave";

const flush = async () => {
  await nextTick();
  await Promise.resolve();
  await nextTick();
};

const harness = <T>(source: Ref<T | null>, save: (v: T) => Promise<void>) =>
  defineComponent({
    setup() {
      const out = useDebouncedSave(source, save, { delayMs: 50, resetMs: 100 });
      return () => h("div", out.state.value);
    },
  });

describe("useDebouncedSave", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("transitions idle → saving → saved → idle on success", async () => {
    const source: Ref<{ v: number } | null> = ref(null);
    const save = vi.fn().mockResolvedValue(undefined);
    const wrapper = mount(harness(source, save));

    source.value = { v: 1 };
    await nextTick();
    expect(wrapper.text()).toBe("saving");

    vi.advanceTimersByTime(50);
    await flush();
    expect(save).toHaveBeenCalledOnce();
    expect(save).toHaveBeenCalledWith({ v: 1 });
    expect(wrapper.text()).toBe("saved");

    vi.advanceTimersByTime(100);
    await flush();
    expect(wrapper.text()).toBe("idle");
  });

  it("debounces rapid changes into a single save", async () => {
    const source: Ref<{ v: number } | null> = ref(null);
    const save = vi.fn().mockResolvedValue(undefined);
    mount(harness(source, save));

    source.value = { v: 1 };
    await nextTick();
    source.value = { v: 2 };
    await nextTick();
    source.value = { v: 3 };
    await nextTick();

    vi.advanceTimersByTime(50);
    await flush();
    expect(save).toHaveBeenCalledOnce();
    expect(save).toHaveBeenCalledWith({ v: 3 });
  });

  it("transitions to error on rejection", async () => {
    const source: Ref<{ v: number } | null> = ref(null);
    const save = vi.fn().mockRejectedValue(new Error("boom"));
    const wrapper = mount(harness(source, save));

    source.value = { v: 1 };
    await nextTick();
    vi.advanceTimersByTime(50);
    await flush();
    expect(wrapper.text()).toBe("error");
  });

  it("serialises overlapping saves and writes the latest value last", async () => {
    const source: Ref<{ v: number } | null> = ref(null);
    const releaseFirst: Array<() => void> = [];
    const saved: Array<{ v: number }> = [];
    const save = vi.fn(async (value: { v: number }) => {
      saved.push(value);
      if (value.v === 1) await new Promise<void>((resolve) => releaseFirst.push(resolve));
    });
    mount(harness(source, save));

    source.value = { v: 1 };
    await nextTick();
    vi.advanceTimersByTime(50);
    await flush();

    source.value = { v: 2 };
    await nextTick();
    vi.advanceTimersByTime(50);
    await flush();
    expect(save).toHaveBeenCalledOnce();

    releaseFirst[0]?.();
    await flush();
    expect(saved).toEqual([{ v: 1 }, { v: 2 }]);
  });
});
