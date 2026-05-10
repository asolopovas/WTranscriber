import { describe, expect, it } from "vitest";
import { detachedSpawnOptions, killProcessTree } from "./process-tree";

describe("process tree helpers", () => {
  it("spawns children as detached process groups", () => {
    expect(detachedSpawnOptions).toEqual({ detached: true });
  });

  it("returns false when there is no pid", () => {
    const killed = killProcessTree({ pid: undefined, kill: () => true });

    expect(killed).toBe(false);
  });
});
