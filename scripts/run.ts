#!/usr/bin/env bun
import { spawn } from "node:child_process";
import { closeSync, mkdirSync, openSync, writeSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { detachedSpawnOptions, killProcessTree } from "./process-tree";

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const ansiRe = /\x1b\[[0-9;]*m/g;
const stripAnsi = (s: string): string => s.replace(ansiRe, "");

interface Options {
  tag: string;
  idle: number;
  max: number;
  cmd: string[];
}

const parseArgs = (argv: string[]): Options => {
  const o: Options = { tag: "task", idle: 90, max: 600, cmd: [] };
  let i = 0;
  while (i < argv.length) {
    const a = argv[i]!;
    if (a === "--") {
      o.cmd = argv.slice(i + 1);
      break;
    }
    if (a === "--tag") o.tag = argv[++i] ?? o.tag;
    else if (a === "--idle") o.idle = Number(argv[++i]);
    else if (a === "--max") o.max = Number(argv[++i]);
    else if (a === "--heartbeat") {
      i++;
    } else {
      console.error(`run.ts: unknown arg ${a}`);
      process.exit(2);
    }
    i++;
  }
  return o;
};

const opts = parseArgs(process.argv.slice(2));
if (opts.cmd.length === 0) {
  console.error("run.ts: no command. usage: --tag NAME [--idle N] [--max N] -- cmd args...");
  process.exit(2);
}

const C = process.stdout.isTTY
  ? { cyan: "\x1b[36m", yellow: "\x1b[33m", red: "\x1b[31m", green: "\x1b[32m", reset: "\x1b[0m" }
  : { cyan: "", yellow: "", red: "", green: "", reset: "" };

const tag = opts.tag;
const start = Date.now();
const prefix = `${C.cyan}[${tag}]${C.reset}`;
const stamp = (): string => ((Date.now() - start) / 1000).toFixed(1) + "s";

const logsDir = join(repoRoot, "logs");
mkdirSync(logsDir, { recursive: true });
const logPath = join(logsDir, `${tag}.log`);
const logFd = openSync(logPath, "w");
const logLine = (line: string): void => {
  try {
    writeSync(logFd, stripAnsi(line));
  } catch {
    /* swallow log write errors */
  }
};
logLine(`# ${new Date().toISOString()} ${opts.cmd.join(" ")}\n`);

const emitStdout = (line: string): void => {
  process.stdout.write(line);
  logLine(line);
};
const emitStderr = (line: string): void => {
  process.stderr.write(line);
  logLine(line);
};

emitStdout(`${prefix} ${C.green}starting${C.reset} ${opts.cmd.join(" ")}\n`);
emitStdout(`${prefix} log: ${logPath}\n`);

let lastOutput = Date.now();
let killReason: string | null = null;

const child = spawn(opts.cmd[0]!, opts.cmd.slice(1), {
  ...detachedSpawnOptions,
  stdio: ["inherit", "pipe", "pipe"],
  env: { ...process.env, FORCE_COLOR: process.env.FORCE_COLOR ?? "1" },
});

emitStdout(`${prefix} spawned pid=${child.pid ?? "?"} idle=${opts.idle}s max=${opts.max}s\n`);

const pipe = (stream: NodeJS.ReadableStream, emit: (line: string) => void): void => {
  let buf = "";
  stream.on("data", (chunk: Buffer | string) => {
    buf += chunk.toString();
    const lines = buf.split(/\r?\n/);
    buf = lines.pop() ?? "";
    for (const ln of lines) {
      lastOutput = Date.now();
      emit(`${prefix} ${ln}\n`);
    }
  });
  stream.on("end", () => {
    if (buf.length) {
      lastOutput = Date.now();
      emit(`${prefix} ${buf}\n`);
    }
  });
};
pipe(child.stdout!, emitStdout);
pipe(child.stderr!, emitStderr);

const watchdog =
  opts.idle > 0
    ? setInterval(() => {
        const idleMs = Date.now() - lastOutput;
        if (idleMs > opts.idle * 1000) {
          killReason = `IDLE_TIMEOUT (${opts.idle}s without output)`;
          emitStderr(`${prefix} ${C.red}FAIL ${killReason} — killing${C.reset}\n`);
          killProcessTree(child, "SIGKILL");
        }
      }, 1000)
    : null;

const hardTimer =
  opts.max > 0
    ? setTimeout(() => {
        killReason = killReason ?? `MAX_TIMEOUT (${opts.max}s)`;
        emitStderr(`${prefix} ${C.red}FAIL ${killReason} — killing${C.reset}\n`);
        killProcessTree(child, "SIGKILL");
      }, opts.max * 1000)
    : null;

const cleanup = (): void => {
  if (watchdog) clearInterval(watchdog);
  if (hardTimer) clearTimeout(hardTimer);
  try {
    closeSync(logFd);
  } catch {
    /* fd may already be closed */
  }
};

for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"] as const) {
  process.on(sig, () => {
    killProcessTree(child, sig);
  });
}

child.on("error", (err: Error) => {
  emitStderr(`${prefix} ${C.red}FAIL spawn: ${err.message}${C.reset}\n`);
  cleanup();
  process.exit(127);
});

child.on("exit", (code: number | null, signal: NodeJS.Signals | null) => {
  if (killReason) {
    emitStderr(`${prefix} ${C.red}FAIL ${killReason} after ${stamp()}${C.reset}\n`);
    cleanup();
    process.exit(124);
  }
  if (code === 0) {
    emitStdout(`${prefix} ${C.green}OK in ${stamp()}${C.reset}\n`);
    cleanup();
    process.exit(0);
  }
  emitStderr(`${prefix} ${C.red}FAIL exit=${code ?? `signal:${signal}`} in ${stamp()}${C.reset}\n`);
  cleanup();
  process.exit(code ?? 1);
});
