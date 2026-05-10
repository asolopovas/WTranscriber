#!/usr/bin/env node

import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const argv = process.argv.slice(2);
const jobs = [];
let idle = 90;
let max = 600;
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  if (a === "--job") jobs.push(argv[++i]);
  else if (a === "--idle") idle = Number(argv[++i]);
  else if (a === "--max") max = Number(argv[++i]);
  else {
    console.error(`parallel.mjs: unknown arg ${a}`);
    process.exit(2);
  }
}
if (!jobs.length) {
  console.error("parallel.mjs: no --job given");
  process.exit(2);
}

const runner = path.join(path.dirname(fileURLToPath(import.meta.url)), "run.mjs");
const isWin = process.platform === "win32";
const start = Date.now();

const procs = jobs.map((spec) => {
  const eq = spec.indexOf("=");
  if (eq < 1) {
    console.error(`parallel.mjs: bad --job spec '${spec}', expected tag=cmd`);
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

let firstFail = null;
await Promise.all(
  procs.map(
    (p) =>
      new Promise((resolve) => {
        p.child.on("exit", (code) => {
          p.code = code ?? 1;
          if (p.code !== 0 && firstFail === null) firstFail = { tag: p.tag, code: p.code };
          resolve();
        });
        p.child.on("error", (err) => {
          p.code = 127;
          if (firstFail === null) firstFail = { tag: p.tag, code: 127, err: err.message };
          resolve();
        });
      }),
  ),
);

const elapsed = ((Date.now() - start) / 1000).toFixed(1);
if (firstFail) {
  process.stderr.write(
    `\x1b[31m[parallel] FAIL ${firstFail.tag} exit=${firstFail.code} (${elapsed}s)\x1b[0m\n`,
  );
  process.exit(firstFail.code);
}
process.stdout.write(`\x1b[32m[parallel] OK ${jobs.length} jobs in ${elapsed}s\x1b[0m\n`);
