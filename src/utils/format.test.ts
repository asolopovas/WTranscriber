import { describe, expect, it } from "vitest";
import { fmtBytes, fmtClock, fmtModelSize, fmtMs, fmtMsLong, MB, progressPct } from "./format";

describe("fmtClock", () => {
  it.each([
    [0, "00:00"],
    [5, "00:05"],
    [65, "01:05"],
    [3599, "59:59"],
    [3600, "1:00:00"],
    [3725, "1:02:05"],
  ])("formats %i seconds as %s", (input, expected) => {
    expect(fmtClock(input)).toBe(expected);
  });

  it("clamps negative and non-finite to zero", () => {
    expect(fmtClock(-10)).toBe("00:00");
    expect(fmtClock(NaN)).toBe("00:00");
    expect(fmtClock(Infinity)).toBe("00:00");
  });
});

describe("fmtMs / fmtMsLong", () => {
  it("formats milliseconds as mm:ss", () => {
    expect(fmtMs(0)).toBe("00:00");
    expect(fmtMs(65_000)).toBe("01:05");
  });
  it("formats milliseconds as hh:mm:ss", () => {
    expect(fmtMsLong(0)).toBe("00:00:00");
    expect(fmtMsLong(3_725_000)).toBe("01:02:05");
  });
});

describe("fmtBytes", () => {
  it.each([
    [0, "0 B"],
    [1023, "1023 B"],
    [1024, "1 KB"],
    [1024 * 1024, "1.0 MB"],
    [1024 * 1024 * 1024, "1.00 GB"],
  ])("formats %i bytes as %s", (input, expected) => {
    expect(fmtBytes(input)).toBe(expected);
  });
});

describe("fmtModelSize", () => {
  it("returns em-dash for zero", () => {
    expect(fmtModelSize(0)).toBe("—");
  });
  it("renders MB below 1 GiB", () => {
    expect(fmtModelSize(500 * MB)).toBe("500 MB");
  });
  it("renders GB at or above 1 GiB", () => {
    expect(fmtModelSize(2 * 1024 * MB)).toBe("2.00 GB");
  });
});

describe("progressPct", () => {
  it("returns 0 for missing or empty totals", () => {
    expect(progressPct()).toBe(0);
    expect(
      progressPct({
        id: "x",
        rel_path: "x",
        file_index: 0,
        file_count: 1,
        downloaded: 0,
        total: 0,
      }),
    ).toBe(0);
  });

  it("computes overall percentage across files", () => {
    const base = { id: "x", rel_path: "x" };
    const half = progressPct({ ...base, file_index: 0, file_count: 2, downloaded: 50, total: 100 });
    expect(half).toBeCloseTo(25, 5);

    const lastFileHalf = progressPct({
      ...base,
      file_index: 1,
      file_count: 2,
      downloaded: 50,
      total: 100,
    });
    expect(lastFileHalf).toBeCloseTo(75, 5);
  });
});
