#!/usr/bin/env bun
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, readFileSync, rmSync, writeFileSync, mkdirSync, readdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

// Auto-recover from a stale cmake build cache when CMAKE_GENERATOR changes
// across runs. cmake-rs reuses target/*/build/<sys-crate>-<hash>/build/
// across cargo invocations; switching generator (e.g. Visual Studio → Ninja)
// leaves CMakeCache.txt pinning the old generator's instance/toolset, and
// subsequent configures crash with "Generator X does not support instance
// specification". src-tauri/build.rs only writes a sentinel — we do the
// actual wipe here, exactly once, before any cargo job starts (doing it from
// build.rs races against the 11 parallel cargo invocations below).
function invalidateStaleCmakeCaches(): void {
  const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const target = path.join(repo, "src-tauri", "target");
  const sentinel = path.join(target, ".cmake-generator");
  const desired = process.env.CMAKE_GENERATOR ?? "";
  let prev = "";
  try {
    prev = readFileSync(sentinel, "utf8");
  } catch {
    prev = "";
  }
  if (prev === desired) return;
  for (const profile of ["debug", "release"]) {
    const buildDir = path.join(target, profile, "build");
    if (!existsSync(buildDir)) continue;
    for (const name of readdirSync(buildDir)) {
      if (name.startsWith("whisper-rs-sys-") || name.startsWith("sherpa-onnx-sys-")) {
        rmSync(path.join(buildDir, name), { recursive: true, force: true });
      }
    }
  }
  mkdirSync(target, { recursive: true });
  writeFileSync(sentinel, desired);
  if (prev) {
    console.error(
      `[parallel] CMAKE_GENERATOR changed (${JSON.stringify(prev)} -> ${JSON.stringify(desired)}); wiped whisper-rs-sys / sherpa-onnx-sys build dirs`,
    );
  }
}
invalidateStaleCmakeCaches();

interface Job {
  tag: string;
  child: ChildProcess;
  code: number | null;
}
interface Failure {
  tag: string;
  code: number;
  err?: string;
}

const argv = process.argv.slice(2);
const jobs: string[] = [];
let idle = 90;
let max = 600;
for (let i = 0; i < argv.length; i++) {
  const a = argv[i]!;
  if (a === "--job") jobs.push(argv[++i]!);
  else if (a === "--idle") idle = Number(argv[++i]);
  else if (a === "--max") max = Number(argv[++i]);
  else {
    console.error(`parallel.ts: unknown arg ${a}`);
    process.exit(2);
  }
}
if (jobs.length === 0) {
  console.error("parallel.ts: no --job given");
  process.exit(2);
}

const runner = path.join(path.dirname(fileURLToPath(import.meta.url)), "run.ts");
const isWin = process.platform === "win32";
const start = Date.now();

const procs: Job[] = jobs.map((spec) => {
  const eq = spec.indexOf("=");
  if (eq < 1) {
    console.error(`parallel.ts: bad --job spec '${spec}', expected tag=cmd`);
    process.exit(2);
  }
  const tag = spec.slice(0, eq);
  const cmdline = spec.slice(eq + 1);
  const shell = isWin ? ["cmd", "/c", cmdline] : ["sh", "-c", cmdline];
  const child = spawn(
    process.execPath,
    [runner, "--tag", tag, "--idle", String(idle), "--max", String(max), "--", ...shell],
    { stdio: "inherit" },
  );
  return { tag, child, code: null };
});

let firstFail: Failure | null = null;
await Promise.all(
  procs.map(
    (p) =>
      new Promise<void>((resolve) => {
        p.child.on("exit", (code: number | null) => {
          p.code = code ?? 1;
          if (p.code !== 0 && firstFail === null) firstFail = { tag: p.tag, code: p.code };
          resolve();
        });
        p.child.on("error", (err: Error) => {
          p.code = 127;
          if (firstFail === null) firstFail = { tag: p.tag, code: 127, err: err.message };
          resolve();
        });
      }),
  ),
);

const elapsed = ((Date.now() - start) / 1000).toFixed(1);
if (firstFail) {
  const f = firstFail as Failure;
  process.stderr.write(`\x1b[31m[parallel] FAIL ${f.tag} exit=${f.code} (${elapsed}s)\x1b[0m\n`);
  process.exit(f.code);
}
process.stdout.write(`\x1b[32m[parallel] OK ${jobs.length} jobs in ${elapsed}s\x1b[0m\n`);
