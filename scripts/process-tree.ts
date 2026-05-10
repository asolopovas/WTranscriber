import { spawnSync } from "node:child_process";
import type { ChildProcess, SpawnOptions } from "node:child_process";

export const detachedSpawnOptions: Pick<SpawnOptions, "detached"> = { detached: true };

export function killProcessTree(
  child: Pick<ChildProcess, "pid" | "kill">,
  signal: NodeJS.Signals = "SIGTERM",
): boolean {
  const pid = child.pid;
  if (!pid) return false;
  if (process.platform === "win32") {
    const result = spawnSync("taskkill", ["/pid", String(pid), "/t", "/f"], {
      stdio: "ignore",
      windowsHide: true,
    });
    if (result.status === 0) return true;
    return child.kill(signal);
  }
  try {
    process.kill(-pid, signal);
    return true;
  } catch {
    return child.kill(signal);
  }
}
