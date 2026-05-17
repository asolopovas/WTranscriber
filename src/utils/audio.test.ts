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
    expect(out.timestamp).toBe("250412_093000");
  });

  it("extracts a compact 4-digit timestamp suffix", () => {
    const out = prettyName("call_20250101_153045.flac");
    expect(out.display).toBe("call");
    expect(out.timestamp).toBe("250101_153045");
  });

  it("extracts a 2-digit timestamp suffix", () => {
    const out = prettyName("memo_250607_142233.mp3");
    expect(out.display).toBe("memo");
    expect(out.timestamp).toBe("250607_142233");
  });

  it("extracts a timestamp embedded mid-name and preserves surrounding text", () => {
    const out = prettyName("260501_094242_team_sync.mp3");
    expect(out.display).toBe("team_sync");
    expect(out.timestamp).toBe("260501_094242");
  });

  it("extracts WhatsApp dotted timestamps", () => {
    const out = prettyName("WhatsApp Audio 2026-05-17 at 14.36.09.opus");
    expect(out.display).toBe("WhatsApp Audio");
    expect(out.timestamp).toBe("260517_143609");
    expect(out.timestampPretty).toBe("14:36:09 17/May/26");
  });

  it("extracts date-only WhatsApp AUD names", () => {
    const out = prettyName("AUD-20260517-WA0001.opus");
    expect(out.display).toBe("AUD WA0001");
    expect(out.timestamp).toBe("260517");
    expect(out.timestampPretty).toBe("17/May/26");
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
