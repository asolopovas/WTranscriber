#!/usr/bin/env bun
import { readdir, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const args = new Set(process.argv.slice(2));
const force = args.has("--force");
const dryRun = args.has("--dry-run");
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(scriptDir, "..");
const home = os.homedir();
const tmpDir = os.tmpdir();
const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

const toWindowsLike = (value: string): string => {
  const normalized = value.replaceAll("\\", "/");
  const msys = normalized.match(/^\/([a-zA-Z])\/(.+)$/);
  if (msys) return `${msys[1]!.toUpperCase()}:/${msys[2]}`;
  return normalized;
};

const projectKey = (value: string): string => {
  const normalized = toWindowsLike(value);
  const drive = normalized.match(/^([a-zA-Z]):\/(.+)$/);
  if (drive) {
    return `${drive[1]!.toUpperCase()}--${drive[2]!.split("/").filter(Boolean).join("-")}`;
  }
  return normalized.replace(/^\/+/, "").split("/").filter(Boolean).join("-");
};

const isSameOrChild = (parent: string, child: string): boolean => {
  const r = path.relative(path.resolve(parent), path.resolve(child));
  return r === "" || (!r.startsWith("..") && !path.isAbsolute(r));
};

const assertAllowed = (target: string): void => {
  const allowedRoots = [
    projectRoot,
    path.join(home, ".pi"),
    path.join(home, ".claude"),
    path.join(home, ".codex"),
    tmpDir,
  ];
  if (!allowedRoots.some((root) => isSameOrChild(root, target))) {
    throw new Error(`refusing to remove path outside allowed roots: ${target}`);
  }
};

const pidAlive = (pid: number): boolean => {
  if (!Number.isInteger(pid) || pid <= 0) return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return Boolean(error && (error as NodeJS.ErrnoException).code === "EPERM");
  }
};

const activeProjectProcesses = async (): Promise<string[]> => {
  const f = Bun.file(path.join(projectRoot, "tmp", "_pids.json"));
  if (!(await f.exists())) return [];
  const pids = (await f.json()) as Record<string, number>;
  return Object.entries(pids)
    .filter(([, pid]) => pidAlive(pid))
    .map(([name, pid]) => `${name}:${pid}`);
};

const dirExists = async (p: string): Promise<boolean> => {
  try {
    return (await stat(p)).isDirectory();
  } catch {
    return false;
  }
};

const existingSessionIds = async (claudeProjectDir: string): Promise<string[]> => {
  if (!(await dirExists(claudeProjectDir))) return [];
  const entries = await readdir(claudeProjectDir, { withFileTypes: true });
  const ids = new Set<string>();
  for (const entry of entries) {
    const name = entry.name.endsWith(".jsonl") ? entry.name.slice(0, -6) : entry.name;
    if (uuidPattern.test(name)) ids.add(name);
  }
  return [...ids].sort();
};

const sizeOf = async (target: string): Promise<number> => {
  const info = await stat(target);
  if (info.isFile()) return info.size;
  let total = 0;
  const entries = await readdir(target, { withFileTypes: true });
  for (const entry of entries) total += await sizeOf(path.join(target, entry.name));
  return total;
};

const formatBytes = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GiB`;
};

interface RemoveResult {
  removed: boolean;
  bytes: number;
}

const pathExists = async (p: string): Promise<boolean> => {
  try {
    await stat(p);
    return true;
  } catch {
    return false;
  }
};

const removeTarget = async (target: string): Promise<RemoveResult> => {
  const resolved = path.resolve(target);
  assertAllowed(resolved);
  if (!(await pathExists(resolved))) return { removed: false, bytes: 0 };
  const bytes = await sizeOf(resolved);
  if (!dryRun) {
    await rm(resolved, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
  }
  return { removed: true, bytes };
};

const main = async (): Promise<void> => {
  const active = await activeProjectProcesses();
  if (active.length > 0 && !force) {
    console.error(
      `refusing to clean while the project dev session is active: ${active.join(", ")}`,
    );
    console.error(
      "run `just android-stop` first, or use `just clean-force` if the pid file is stale",
    );
    process.exit(2);
  }

  const key = projectKey(projectRoot);
  const claudeProjectDir = path.join(home, ".claude", "projects", key);
  const sessionIds = await existingSessionIds(claudeProjectDir);
  const targets = new Set<string>([
    path.join(projectRoot, "tmp"),
    path.join(projectRoot, ".playwright-cli"),
    path.join(home, ".pi", "agent", "sessions", `--${key}--`),
    claudeProjectDir,
  ]);

  for (const id of sessionIds) {
    targets.add(path.join(home, ".claude", "file-history", id));
    targets.add(path.join(home, ".claude", "session-env", id));
    targets.add(path.join(home, ".claude", "tasks", id));
  }

  let removed = 0;
  let bytes = 0;
  for (const target of [...targets].sort()) {
    const result = await removeTarget(target);
    if (result.removed) {
      removed += 1;
      bytes += result.bytes;
      console.log(
        `${dryRun ? "would remove" : "removed"} ${target} (${formatBytes(result.bytes)})`,
      );
    }
  }

  if (removed === 0) {
    console.log("no project temp/session files found");
    return;
  }
  console.log(`${dryRun ? "would remove" : "removed"} ${removed} path(s), ${formatBytes(bytes)}`);
};

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
