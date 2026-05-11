import { randomUUID } from "node:crypto";
import { spawn } from "node:child_process";
import { rm, stat } from "node:fs/promises";
import path from "node:path";
import { describe, expect, it } from "vitest";

const collect = (child: ReturnType<typeof spawn>) =>
  new Promise<{ code: number | null; stdout: string; stderr: string }>((resolve, reject) => {
    let stdout = "";
    let stderr = "";
    child.stdout?.on("data", (chunk) => {
      stdout += chunk.toString();
    });
    child.stderr?.on("data", (chunk) => {
      stderr += chunk.toString();
    });
    child.on("error", reject);
    child.on("exit", (code) => resolve({ code, stdout, stderr }));
  });

const exists = async (file: string) => {
  try {
    await stat(file);
    return true;
  } catch {
    return false;
  }
};

describe("parallel.ts", () => {
  it.skipIf(process.platform === "win32")(
    "returns the first failure after all jobs finish",
    async () => {
      const dir = path.join("tmp", `parallel-test-${process.pid}-${randomUUID()}`);
      const ok = path.join(dir, "ok");
      const fail = path.join(dir, "fail");
      await rm(dir, { force: true, recursive: true });

      const child = spawn(
        "bun",
        [
          "scripts/parallel.ts",
          "--idle",
          "10",
          "--max",
          "30",
          "--job",
          `fail=mkdir -p ${dir} && printf fail > ${fail} && exit 7`,
          "--job",
          `ok=mkdir -p ${dir} && printf ok > ${ok}`,
        ],
        { stdio: ["ignore", "pipe", "pipe"] },
      );

      const result = await collect(child);

      expect(result.code).toBe(7);
      expect(result.stderr).toContain("[parallel] FAIL fail exit=7");
      expect(await exists(fail)).toBe(true);
      expect(await exists(ok)).toBe(true);

      await rm(dir, { force: true, recursive: true });
    },
  );
});
