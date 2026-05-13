#!/usr/bin/env bun
import { readdir, rm, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const tmp = join(root, "tmp");
const logs = join(root, "logs");

const pidAlive = (pid: number): boolean => {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    return Boolean(e && (e as NodeJS.ErrnoException).code === "EPERM");
  }
};

const exists = async (p: string): Promise<boolean> => {
  try {
    await stat(p);
    return true;
  } catch {
    return false;
  }
};

const sessionAlive = async (): Promise<boolean> => {
  const f = join(tmp, "_pids.json");
  if (!(await exists(f))) return false;
  try {
    const pids = JSON.parse(await Bun.file(f).text()) as Record<string, number | null>;
    for (const [k, v] of Object.entries(pids)) {
      if (k === "device" || k === "lldb_port") continue;
      if (typeof v === "number" && pidAlive(v)) return true;
    }
  } catch {}
  return false;
};

const wipeFile = async (p: string): Promise<boolean> => {
  if (!(await exists(p))) return false;
  await rm(p, { force: true });
  return true;
};

const wipeDirContents = async (
  dir: string,
  predicate: (name: string) => boolean,
): Promise<number> => {
  if (!(await exists(dir))) return 0;
  let n = 0;
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    if (!entry.isFile()) continue;
    if (!predicate(entry.name)) continue;
    await rm(join(dir, entry.name), { force: true });
    n += 1;
  }
  return n;
};

const main = async (): Promise<void> => {
  const alive = await sessionAlive();
  const removed: string[] = [];

  const logCount = await wipeDirContents(tmp, (n) => n.endsWith(".log"));
  if (logCount) removed.push(`tmp/*.log (${logCount})`);

  const buildLogCount = await wipeDirContents(logs, (n) => n.endsWith(".log"));
  if (buildLogCount) removed.push(`logs/*.log (${buildLogCount})`);

  if (!alive) {
    if (await wipeFile(join(tmp, "_pids.json"))) removed.push("tmp/_pids.json (stale)");
    if (await wipeFile(join(tmp, "_platform"))) removed.push("tmp/_platform (stale)");
    if (await wipeFile(join(tmp, "emulator.pid"))) removed.push("tmp/emulator.pid (stale)");
  } else {
    console.log("dev session alive — kept tmp/_pids.json");
  }

  if (removed.length === 0) {
    console.log("clear-dev-logs: nothing to remove");
    return;
  }
  console.log(`clear-dev-logs: ${removed.join(", ")}`);
};

main().catch((e: unknown) => {
  console.error(e instanceof Error ? e.message : String(e));
  process.exit(1);
});
