import { describe, expect, it } from "vitest";
import { detachedSpawnOptions, killProcessTree } from "./process-tree";

describe("process tree helpers", () => {
  it("spawns children as detached process groups (POSIX) or default groups (Windows)", () => {
    const expected = process.platform === "win32" ? {} : { detached: true };
    expect(detachedSpawnOptions).toEqual(expected);
  });

  it("returns false when there is no pid", () => {
    const killed = killProcessTree({ pid: undefined, kill: () => true });

    expect(killed).toBe(false);
  });
});
