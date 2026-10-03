import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "@/api";
import type { DirEntry } from "@/types";
import TrimDialog from "./TrimDialog.vue";

vi.mock("@/api", () => ({
  api: {
    probeAudio: vi.fn(),
    loadAudioMeta: vi.fn(),
    audioWaveform: vi.fn(),
    readAudioBytes: vi.fn(),
    saveAudioMeta: vi.fn(),
  },
}));

const target: DirEntry = {
  path: "/audio.wav",
  name: "audio.wav",
  is_dir: false,
  is_audio: true,
  size_bytes: 100,
  modified_ms: 1,
  cache_key: null,
  utterances: null,
  duration_ms: 60000,
  trim_start_ms: null,
  trim_end_ms: null,
};

let wrapper: VueWrapper;

async function open() {
  wrapper = mount(TrimDialog, { props: { target }, attachTo: document.body });
  await flushPromises();
  return wrapper.get("audio").element as HTMLAudioElement;
}

function press(key: string, element: EventTarget = window, init: KeyboardEventInit = {}) {
  const ev = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true, ...init });
  element.dispatchEvent(ev);
  return ev;
}

beforeEach(() => {
  vi.resetAllMocks();
  localStorage.clear();
  vi.mocked(api.probeAudio).mockResolvedValue(60000);
  vi.mocked(api.loadAudioMeta).mockResolvedValue({
    trim_start_ms: 0,
    trim_end_ms: null,
    duration_ms: null,
  });
  vi.mocked(api.audioWaveform).mockResolvedValue([0.2, 0.8]);
  vi.mocked(api.readAudioBytes).mockResolvedValue(new ArrayBuffer(0));
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => {});
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:test");
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
});

afterEach(() => {
  wrapper?.unmount();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("trim editor seeking", () => {
  it("seeks by five seconds on initial open, supports repeat, and clamps to track bounds", async () => {
    const audio = await open();
    expect(press("ArrowLeft").defaultPrevented).toBe(true);
    expect(audio.currentTime).toBe(0);
    press("ArrowRight");
    expect(audio.currentTime).toBe(5);
    press("ArrowRight", window, { repeat: true });
    expect(audio.currentTime).toBe(10);
    for (let i = 0; i < 20; i += 1) press("ArrowRight");
    expect(audio.currentTime).toBe(60);
    press("ArrowLeft");
    expect(audio.currentTime).toBe(55);
    await flushPromises();
    expect(wrapper.text()).toContain("Position 00:55");
  });

  it("persists a configurable step and falls back to five seconds for invalid input", async () => {
    let audio = await open();
    await wrapper.get("input").setValue("2.5");
    press("ArrowRight");
    expect(audio.currentTime).toBe(2.5);
    expect(localStorage.getItem("wt.trimSeekStepSeconds")).toBe("2.5");
    wrapper.unmount();
    audio = await open();
    press("ArrowRight");
    expect(audio.currentTime).toBe(2.5);
    for (const value of ["", "0", "-5", "Infinity"]) {
      await wrapper.get("input").setValue(value);
      expect((wrapper.get("input").element as HTMLInputElement).value).toBe("5");
    }
  });

  it("does not intercept editing, selection, or modified shortcuts", async () => {
    const audio = await open();
    for (const tag of ["input", "textarea", "select"]) {
      const element = document.createElement(tag);
      document.body.append(element);
      expect(press("ArrowRight", element).defaultPrevented).toBe(false);
    }
    const editable = document.createElement("div");
    editable.contentEditable = "true";
    const child = document.createElement("span");
    editable.append(child);
    document.body.append(editable);
    expect(press("ArrowRight", child).defaultPrevented).toBe(false);
    for (const modifier of ["ctrlKey", "metaKey", "altKey"]) {
      expect(press("ArrowRight", window, { [modifier]: true }).defaultPrevented).toBe(false);
    }
    expect(audio.currentTime).toBe(0);
  });

  it("seeks from the actual playback time without interrupting playback inside the selection", async () => {
    const audio = await open();
    await wrapper.get('[title="Play selection (Space)"]').trigger("click");
    audio.currentTime = 12;
    const pauseCalls = vi.mocked(audio.pause).mock.calls.length;
    press("ArrowRight");
    expect(audio.currentTime).toBe(17);
    expect(audio.pause).toHaveBeenCalledTimes(pauseCalls);
    audio.currentTime = 59;
    press("ArrowRight");
    expect(audio.currentTime).toBe(60);
    expect(audio.pause).toHaveBeenCalledTimes(pauseCalls + 1);
  });

  it("allows positioning outside a saved selection while staying within the track", async () => {
    vi.mocked(api.loadAudioMeta).mockResolvedValue({
      trim_start_ms: 10000,
      trim_end_ms: 20000,
      duration_ms: null,
    });
    const audio = await open();
    press("ArrowLeft");
    expect(audio.currentTime).toBe(5);
    for (let i = 0; i < 4; i += 1) press("ArrowRight");
    expect(audio.currentTime).toBe(25);
  });

  it("removes shortcuts on close and restores them on reopen", async () => {
    await open();
    await wrapper.setProps({ target: null });
    expect(press("ArrowRight").defaultPrevented).toBe(false);
    await wrapper.setProps({ target });
    await flushPromises();
    press("ArrowRight");
    expect((wrapper.get("audio").element as HTMLAudioElement).currentTime).toBe(5);
  });

  it("ignores preparation results after the editor closes", async () => {
    let resolve!: (peaks: number[]) => void;
    vi.mocked(api.audioWaveform).mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    wrapper = mount(TrimDialog, { props: { target } });
    expect(press("ArrowRight").defaultPrevented).toBe(false);
    await wrapper.setProps({ target: null });
    resolve([0.1]);
    await flushPromises();
    expect(api.readAudioBytes).not.toHaveBeenCalled();
    expect(wrapper.emitted("error")).toBeUndefined();
  });

  it("does not create a blob URL for an audio load completed after closing", async () => {
    let resolve!: (bytes: ArrayBuffer) => void;
    vi.mocked(api.readAudioBytes).mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    wrapper = mount(TrimDialog, { props: { target } });
    await flushPromises();
    await wrapper.setProps({ target: null });
    resolve(new ArrayBuffer(0));
    await flushPromises();
    expect(URL.createObjectURL).not.toHaveBeenCalled();
  });
});
