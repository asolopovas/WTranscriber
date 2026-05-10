import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import { describe, expect, it } from "vitest";

const collect = (child: ReturnType<typeof spawn>) =>
  new Promise<{ code: number | null; stderr: string }>((resolve, reject) => {
    let stderr = "";
    child.stderr?.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("exit", (code) => resolve({ code, stderr }));
  });

const pgrep = (pattern: string) =>
  collect(spawn("pgrep", ["-f", pattern], { stdio: ["ignore", "ignore", "pipe"] }));

describe("run.ts", () => {
  it.skipIf(process.platform === "win32")("kills shell grandchildren on timeout", async () => {
    const marker = `wt-run-tree-${process.pid}-${Date.now()}`;
    const child = spawn(
      "bun",
      [
        "scripts/run.ts",
        "--tag",
        "tree-test",
        "--idle",
        "0",
        "--max",
        "1",
        "--",
        "bash",
        "-lc",
        `exec -a ${marker} sleep 30 & wait`,
      ],
      { stdio: ["ignore", "ignore", "pipe"] },
    );

    const result = await collect(child);
    await delay(200);
    const survivors = await pgrep(marker);

    expect(result.code).toBe(124);
    expect(result.stderr).toContain("MAX_TIMEOUT");
    expect(survivors.code).toBe(1);
  }, 10_000);
});
