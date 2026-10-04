import { flushPromises, mount, type VueWrapper } from "@vue/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "@/api";
import type { Config, Transcript } from "@/types";
import TranscriptPanel from "./TranscriptPanel.vue";

vi.mock("@/api", () => ({
  api: {
    transcriptCanUndo: vi.fn(),
    undoTranscriptEdit: vi.fn(),
    deleteTranscriptSegment: vi.fn(),
    markTranscriptReview: vi.fn(),
    retryTranscriptSegment: vi.fn(),
    renameSpeaker: vi.fn(),
    setTranscriptSpeaker: vi.fn(),
    replaceTranscriptText: vi.fn(),
    updateTranscriptText: vi.fn(),
    readAudioSegment: vi.fn(),
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
  });
}
beforeEach(() => {
  vi.resetAllMocks();
  localStorage.clear();
  vi.mocked(api.transcriptCanUndo).mockResolvedValue(false);
  frames = [];
  vi.mocked(api.readAudioSegment).mockResolvedValue(new ArrayBuffer(0));
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "readyState", "get").mockReturnValue(4);
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
  it("offers timing review without changing segment indices or labelling other engines", async () => {
    open();
    await wrapper.setProps({
      transcript: {
        ...transcript,
        diarizer: "whisperx-community-1-v1",
        words: [
          { text: "First.", start_ms: 1000, end_ms: 2000, speaker: "Speaker", confidence: 0.9 },
          { text: "Second.", start_ms: 3000, end_ms: 4000, speaker: "Speaker", confidence: 0.2 },
        ],
      },
    });
    expect(wrapper.get("summary").text()).toBe("Timing review: 1 of 2 words");
    const review = wrapper
      .findAll("button")
      .find((button) => button.text() === "Show 1 segment to review")!;
    await review.trigger("click");
    expect(wrapper.findAll("article")).toHaveLength(1);
    expect(wrapper.get("article").text()).toContain("Lower-scoring words: Second.");
    await wrapper.get('[title="Edit segment 2"]').trigger("click");
    expect(wrapper.get("textarea").element.value).toBe("Second.");
    await wrapper.setProps({
      transcript: { ...transcript, diarizer: "sortformer" },
      sourcePath: "/other.wav",
    });
    expect(wrapper.find("summary").exists()).toBe(false);
    expect(wrapper.findAll("article")).toHaveLength(2);
  });

  it("changes the speaker on one segment without renaming every occurrence", async () => {
    open();
    await wrapper.get('[title="Rename speaker"]').trigger("click");
    expect(wrapper.get("select").element.value).toBe("segment");
    expect(wrapper.text()).not.toContain("Change only segment");
    await wrapper.get("input").setValue("Alice");
    vi.mocked(api.setTranscriptSpeaker).mockResolvedValueOnce(transcript);
    await wrapper.get("input").trigger("keydown", { key: "Enter" });
    await flushPromises();
    expect(api.setTranscriptSpeaker).toHaveBeenCalledWith("key", "/audio.wav", 0, "Alice");
    expect(api.renameSpeaker).not.toHaveBeenCalled();
  });

  it("previews replacements across the transcript and retains a failed draft", async () => {
    open();
    await wrapper.get('[title="Find and replace"]').trigger("click");
    const fields = wrapper.findAll("input");
    await fields[0].setValue(".");
    await fields[1].setValue("!");
    expect(wrapper.text()).toContain("2 matches in 2 segments");
    const replace = () =>
      wrapper.findAll("button").find((button) => button.text() === "Replace all")!;
    vi.mocked(api.replaceTranscriptText).mockRejectedValueOnce(new Error("disk full"));
    await replace().trigger("click");
    await flushPromises();
    expect(wrapper.text()).toContain("Could not replace text");
    expect((fields[0].element as HTMLInputElement).value).toBe(".");
    vi.mocked(api.replaceTranscriptText).mockResolvedValueOnce(transcript);
    await replace().trigger("click");
    await flushPromises();
    expect(api.replaceTranscriptText).toHaveBeenLastCalledWith("key", "/audio.wav", ".", "!");
    expect(wrapper.emitted("updated")).toHaveLength(1);
  });
  it("renames through a modal, retaining the draft if saving fails", async () => {
    open();
    await wrapper.get('[title="Rename speaker"]').trigger("click");
    await wrapper.get("select").setValue("all");
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
  it("persists deletion and undoes it with Ctrl+Z, retaining history after a failed undo", async () => {
    open();
    const deleted = { ...transcript, utterances: transcript.utterances.slice(1) };
    vi.mocked(api.deleteTranscriptSegment).mockResolvedValueOnce(deleted);
    vi.mocked(api.transcriptCanUndo).mockResolvedValue(true);
    await wrapper.get('[title="Delete segment 1"]').trigger("click");
    await flushPromises();
    await wrapper.setProps({ transcript: deleted });
    expect(api.deleteTranscriptSegment).toHaveBeenCalledWith("key", "/audio.wav", 0);
    vi.mocked(api.undoTranscriptEdit).mockRejectedValueOnce(new Error("disk full"));
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "z", ctrlKey: true, cancelable: true }),
    );
    await flushPromises();
    expect(wrapper.text()).toContain("Could not undo edit");
    vi.mocked(api.undoTranscriptEdit).mockResolvedValueOnce(transcript);
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "z", ctrlKey: true, cancelable: true }),
    );
    await flushPromises();
    expect(api.undoTranscriptEdit).toHaveBeenCalledTimes(2);
    expect(wrapper.emitted("updated")?.slice(-1)[0]).toEqual([transcript]);
  });

  it("keeps native text undo inside fields and ignores repeated or composing shortcuts", async () => {
    vi.mocked(api.transcriptCanUndo).mockResolvedValue(true);
    open();
    await flushPromises();
    await wrapper.get('[title="Edit segment 1"]').trigger("click");
    await wrapper.get("textarea").trigger("keydown", { key: "z", ctrlKey: true });
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, repeat: true }));
    window.dispatchEvent(
      new KeyboardEvent("keydown", { key: "z", ctrlKey: true, isComposing: true }),
    );
    expect(api.undoTranscriptEdit).not.toHaveBeenCalled();
  });

  it("includes manual marks in review filtering and targets original segment indices", async () => {
    open();
    const marked = {
      ...transcript,
      utterances: transcript.utterances.map((u, i) => ({ ...u, needs_review: i === 1 })),
    };
    vi.mocked(api.markTranscriptReview).mockResolvedValueOnce(marked);
    await wrapper.get('[title="Mark for review: segment 2"]').trigger("click");
    await flushPromises();
    await wrapper.setProps({ transcript: marked });
    expect(api.markTranscriptReview).toHaveBeenCalledWith("key", "/audio.wav", 1, true);
    await wrapper
      .findAll("button")
      .find((button) => button.text() === "Show 1 segment to review")!
      .trigger("click");
    expect(wrapper.findAll("article")).toHaveLength(1);
    vi.mocked(api.markTranscriptReview).mockResolvedValueOnce(transcript);
    await wrapper.get('[title="Clear review mark for segment 2"]').trigger("click");
    await flushPromises();
    expect(api.markTranscriptReview).toHaveBeenLastCalledWith("key", "/audio.wav", 1, false);
  });

  it("retries recognition only for the chosen segment and retains the transcript on failure", async () => {
    open();
    const config = { model: "test" } as Config;
    await wrapper.setProps({ config });
    vi.mocked(api.retryTranscriptSegment).mockRejectedValueOnce(new Error("no speech"));
    await wrapper.get('[title="Retry recognition for segment 2"]').trigger("click");
    await flushPromises();
    expect(api.retryTranscriptSegment).toHaveBeenCalledWith("key", "/audio.wav", 1, config);
    expect(wrapper.text()).toContain("Could not retry recognition");
    expect(wrapper.emitted("updated")).toBeUndefined();
    expect(wrapper.findAll("article")).toHaveLength(2);
    vi.mocked(api.retryTranscriptSegment).mockResolvedValueOnce(transcript);
    await wrapper.get('[title="Retry recognition for segment 2"]').trigger("click");
    await flushPromises();
    expect(wrapper.emitted("updated")).toEqual([[transcript]]);
  });

  it("requests the original range, plays a bounded WAV, and reuses cached fragments", async () => {
    localStorage.setItem("wt.transcriptPlaybackContext", "false");
    open();
    await wrapper.get('[title="Play segment 2"]').trigger("click");
    await flushPromises();
    const audio = wrapper.get("audio").element as HTMLAudioElement;
    expect(api.readAudioSegment).toHaveBeenCalledWith("/audio.wav", 3000, 4000);
    expect(audio.currentTime).toBe(0);
    audio.currentTime = 1;
    frames[frames.length - 1]?.(0);
    await flushPromises();
    expect(wrapper.find('[title="Play segment 2"]').exists()).toBe(true);
    expect(audio.pause).toHaveBeenCalled();
    await wrapper.get('[title="Play segment 1"]').trigger("click");
    await flushPromises();
    expect((wrapper.get("audio").element as HTMLAudioElement).currentTime).toBe(0);
    expect(api.readAudioSegment).toHaveBeenCalledTimes(2);
  });
  it("ignores a pending audio read after the viewer closes", async () => {
    let resolve!: (value: ArrayBuffer) => void;
    vi.mocked(api.readAudioSegment).mockReturnValue(
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

  it("ignores initial time updates and waits for canplay before starting", async () => {
    vi.spyOn(HTMLMediaElement.prototype, "readyState", "get").mockReturnValue(0);
    open();
    await wrapper.get('[title="Play segment 1"]').trigger("click");
    await flushPromises();
    await wrapper.get("audio").trigger("timeupdate");
    expect(wrapper.find('[title="Stop segment 1"]').exists()).toBe(true);
    expect(HTMLMediaElement.prototype.play).not.toHaveBeenCalled();
    await wrapper.get("audio").trigger("canplay");
    await flushPromises();
    expect(HTMLMediaElement.prototype.play).toHaveBeenCalledTimes(1);
  });
});
