import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "@/api";
import type { Transcript } from "@/types";
import TranscriptPanel from "./TranscriptPanel.vue";

vi.mock("@/api", () => ({
  api: {
    renameSpeaker: vi.fn(),
    updateTranscriptText: vi.fn(),
    readAudioBytes: vi.fn(),
    formatTranscript: vi.fn(),
  },
}));
const transcript: Transcript = {
  model: "test",
  language: "en",
  duration_ms: 10000,
  speakers_detected: 1,
  utterances: [
    { start_ms: 1000, end_ms: 2000, speaker: "Speaker", text: "First." },
    { start_ms: 3000, end_ms: 4000, speaker: "Speaker", text: "Second." },
  ],
  words: [],
};
let wrapper: VueWrapper;
let frames: FrameRequestCallback[];
function open() {
  wrapper = mount(TranscriptPanel, {
    props: { transcript, cacheKey: "key", sourcePath: "/audio.wav" },
    attachTo: document.body,
    global: { stubs: { SlidingPanel: { template: '<div><slot name="header"/><slot/></div>' } } },
  });
}
beforeEach(() => {
  vi.resetAllMocks();
  frames = [];
  vi.mocked(api.readAudioBytes).mockResolvedValue(new ArrayBuffer(0));
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:test");
  vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.push(callback);
    return frames.length;
  });
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
});
afterEach(() => {
  wrapper?.unmount();
  document.body.innerHTML = "";
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("TranscriptPanel", () => {
  it("renames through a modal, retaining the draft if saving fails", async () => {
    open();
    await wrapper.get('[title="Rename speaker"]').trigger("click");
    const input = wrapper.get("input");
    expect(document.activeElement).toBe(input.element);
    await input.setValue("Alice");
    vi.mocked(api.renameSpeaker).mockRejectedValueOnce(new Error("disk full"));
    await input.trigger("keydown", { key: "Enter" });
    await flushPromises();
    expect(wrapper.text()).toContain("Could not save speaker name");
    expect((input.element as HTMLInputElement).value).toBe("Alice");
    vi.mocked(api.renameSpeaker).mockResolvedValueOnce({
      ...transcript,
      utterances: transcript.utterances.map((u) => ({ ...u, speaker: "Alice" })),
    });
    await input.trigger("keydown", { key: "Enter" });
    await flushPromises();
    expect(api.renameSpeaker).toHaveBeenLastCalledWith("key", "Speaker", "Alice", "/audio.wav");
    const updated = wrapper.emitted("updated")?.[0]?.[0] as Transcript;
    expect(updated.utterances.every((u) => u.speaker === "Alice")).toBe(true);
  });
  it("keeps a failed text edit available for retry", async () => {
    open();
    await wrapper.get('[title="Edit segment 1"]').trigger("click");
    await wrapper.get("textarea").setValue("Correction.");
    vi.mocked(api.updateTranscriptText).mockRejectedValueOnce(new Error("read only"));
    await wrapper.get("textarea").trigger("keydown", { key: "Enter", ctrlKey: true });
    await flushPromises();
    expect(wrapper.text()).toContain("Could not save text");
    expect((wrapper.get("textarea").element as HTMLTextAreaElement).value).toBe("Correction.");
    vi.mocked(api.updateTranscriptText).mockResolvedValueOnce(transcript);
    await wrapper.get("textarea").trigger("keydown", { key: "Enter", ctrlKey: true });
    await flushPromises();
    expect(api.updateTranscriptText).toHaveBeenLastCalledWith(
      "key",
      "/audio.wav",
      0,
      "Correction.",
    );
    expect(wrapper.find("textarea").exists()).toBe(false);
  });
  it("uses absolute segment times, stops at the end, and reuses the audio blob", async () => {
    open();
    await wrapper.get('[title="Play segment 2"]').trigger("click");
    await flushPromises();
    const audio = wrapper.get("audio").element as HTMLAudioElement;
    expect(audio.currentTime).toBe(3);
    audio.currentTime = 4;
    frames[frames.length - 1]?.(0);
    await flushPromises();
    expect(wrapper.find('[title="Play segment 2"]').exists()).toBe(true);
    expect(audio.pause).toHaveBeenCalled();
    await wrapper.get('[title="Play segment 1"]').trigger("click");
    await flushPromises();
    expect(audio.currentTime).toBe(1);
    expect(api.readAudioBytes).toHaveBeenCalledTimes(1);
  });
  it("ignores a pending audio read after the viewer closes", async () => {
    let resolve!: (value: ArrayBuffer) => void;
    vi.mocked(api.readAudioBytes).mockReturnValue(
      new Promise((done) => {
        resolve = done;
      }),
    );
    open();
    await wrapper.get('[title="Play segment 1"]').trigger("click");
    wrapper.unmount();
    resolve(new ArrayBuffer(0));
    await flushPromises();
    expect(URL.createObjectURL).not.toHaveBeenCalled();
    expect(HTMLMediaElement.prototype.play).not.toHaveBeenCalled();
  });
});
