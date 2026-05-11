import { describe, expect, it } from "vitest";
import { basenameOf, decodeName, hasAudioExt, phaseLabel, prettyName } from "./audio";

describe("hasAudioExt", () => {
  it.each([
    "a.wav",
    "a.MP3",
    "deep/path/to.flac",
    "x.OPUS",
    "WhatsApp Audio 2026-05-11 at 10.41.36.mp4",
    "voice.3gp",
    "voice.amr",
    "clip.mov",
    "meeting.MKV",
    "x.aiff",
    "y.caf",
    "track.m4b",
    "podcast.oga",
  ])("accepts %s", (p) => {
    expect(hasAudioExt(p)).toBe(true);
  });
  it.each(["a.txt", "a", "a.zip", ""])("rejects %s", (p) => {
    expect(hasAudioExt(p)).toBe(false);
  });
});

describe("basenameOf", () => {
  it("returns the file name from POSIX paths", () => {
    expect(basenameOf("/tmp/recordings/clip.wav")).toBe("clip.wav");
  });
  it("normalises Windows separators", () => {
    expect(basenameOf("C:\\Users\\me\\clip.wav")).toBe("clip.wav");
  });
  it("strips file:// scheme and url-decodes", () => {
    expect(basenameOf("file:///home/user/My%20Clip.wav")).toBe("My Clip.wav");
  });
  it("drops query strings", () => {
    expect(basenameOf("https://x/y/clip.wav?token=1")).toBe("clip.wav");
  });
  it("falls back to 'audio' for empty results", () => {
    expect(basenameOf("/")).toBe("audio");
  });
});

describe("decodeName", () => {
  it("decodes percent-encoded names", () => {
    expect(decodeName("My%20Clip.wav")).toBe("My Clip.wav");
  });
  it("returns input unchanged on malformed encoding", () => {
    expect(decodeName("100%off.wav")).toBe("100%off.wav");
  });
});

describe("prettyName", () => {
  it("extracts a 4-digit ISO timestamp suffix", () => {
    const out = prettyName("meeting_2025-04-12T09-30-00.wav");
    expect(out.display).toBe("meeting");
    expect(out.timestamp).toBe("25-Apr-12 09:30");
  });

  it("extracts a compact 4-digit timestamp suffix", () => {
    const out = prettyName("call_20250101_153045.flac");
    expect(out.display).toBe("call");
    expect(out.timestamp).toBe("25-Jan-01 15:30");
  });

  it("extracts a 2-digit timestamp suffix", () => {
    const out = prettyName("memo_250607_142233.mp3");
    expect(out.display).toBe("memo");
    expect(out.timestamp).toBe("25-Jun-07 14:22");
  });

  it("returns null timestamp when no pattern matches", () => {
    const out = prettyName("plain-name.wav");
    expect(out.display).toBe("plain-name");
    expect(out.timestamp).toBeNull();
  });
});

describe("phaseLabel", () => {
  it.each([
    ["cache_check", "checking cache"],
    ["transcribing", "transcribing"],
    ["done", "done"],
  ] as const)("maps %s to %s", (phase, label) => {
    expect(phaseLabel(phase)).toBe(label);
  });
});
