import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
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

const isAlive = (pid: number) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

describe("run.ts", () => {
  it("kills grandchildren on timeout", async () => {
    const dir = mkdtempSync(join(tmpdir(), "wt-run-tree-"));
    const helperPath = join(dir, "helper.mjs");
    const pidPath = join(dir, "grandchild.pid");
    writeFileSync(
      helperPath,
      `import { spawn } from "node:child_process";\nimport { writeFileSync } from "node:fs";\nconst child = spawn(process.execPath, ["-e", "setInterval(() => {}, 30000)"], { stdio: "ignore" });\nchild.unref();\nwriteFileSync(process.argv[2], String(child.pid ?? ""));\nsetInterval(() => {}, 30000);\n`,
    );

    try {
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
          "bun",
          helperPath,
          pidPath,
        ],
        { stdio: ["ignore", "ignore", "pipe"] },
      );

      const result = await collect(child);
      await delay(200);
      const grandchildPid = Number(readFileSync(pidPath, "utf8"));

      expect(result.code).toBe(124);
      expect(result.stderr).toContain("MAX_TIMEOUT");
      expect(Number.isInteger(grandchildPid)).toBe(true);
      expect(isAlive(grandchildPid)).toBe(false);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  }, 10_000);
});
