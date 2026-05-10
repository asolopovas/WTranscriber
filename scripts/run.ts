#!/usr/bin/env bun
import { spawn } from "node:child_process";
import { detachedSpawnOptions, killProcessTree } from "./process-tree";

interface Options {
  tag: string;
  idle: number;
  max: number;
  heartbeat: number;
  cmd: string[];
}

const parseArgs = (argv: string[]): Options => {
  const o: Options = { tag: "task", idle: 90, max: 600, heartbeat: 10, cmd: [] };
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
    else if (a === "--heartbeat") o.heartbeat = Number(argv[++i]);
    else {
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

let lastOutput = Date.now();
let lastLine = "";
let killReason: string | null = null;

const child = spawn(opts.cmd[0]!, opts.cmd.slice(1), {
  ...detachedSpawnOptions,
  stdio: ["inherit", "pipe", "pipe"],
  env: { ...process.env, FORCE_COLOR: process.env.FORCE_COLOR ?? "1" },
});

const pipe = (stream: NodeJS.ReadableStream, sink: NodeJS.WritableStream): void => {
  let buf = "";
  stream.on("data", (chunk: Buffer | string) => {
    buf += chunk.toString();
    const lines = buf.split(/\r?\n/);
    buf = lines.pop() ?? "";
    for (const ln of lines) {
      lastOutput = Date.now();
      lastLine = ln;
      sink.write(`${prefix} ${ln}\n`);
    }
  });
  stream.on("end", () => {
    if (buf.length) {
      lastOutput = Date.now();
      lastLine = buf;
      sink.write(`${prefix} ${buf}\n`);
    }
  });
};
pipe(child.stdout!, process.stdout);
pipe(child.stderr!, process.stderr);

const heartbeat = setInterval(() => {
  const idleMs = Date.now() - lastOutput;
  if (idleMs >= opts.heartbeat * 1000) {
    const tail = lastLine ? ` (last: ${lastLine.slice(0, 80)})` : "";
    process.stderr.write(
      `${prefix} ${C.yellow}… still running, ${stamp()} elapsed, ${(idleMs / 1000).toFixed(0)}s without output${tail}${C.reset}\n`,
    );
  }
}, Math.max(1000, opts.heartbeat * 1000));

const idleTimer = setInterval(() => {
  if (opts.idle <= 0) return;
  if (Date.now() - lastOutput > opts.idle * 1000) {
    killReason = `IDLE_TIMEOUT (${opts.idle}s without output)`;
    process.stderr.write(`${prefix} ${C.red}FAIL ${killReason} — killing${C.reset}\n`);
    killProcessTree(child, "SIGKILL");
  }
}, 1000);

const hardTimer =
  opts.max > 0
    ? setTimeout(() => {
        killReason = killReason ?? `MAX_TIMEOUT (${opts.max}s)`;
        process.stderr.write(`${prefix} ${C.red}FAIL ${killReason} — killing${C.reset}\n`);
        killProcessTree(child, "SIGKILL");
      }, opts.max * 1000)
    : null;

const cleanup = (): void => {
  clearInterval(heartbeat);
  clearInterval(idleTimer);
  if (hardTimer) clearTimeout(hardTimer);
};

for (const sig of ["SIGINT", "SIGTERM", "SIGHUP"] as const) {
  process.on(sig, () => {
    killProcessTree(child, sig);
  });
}

child.on("error", (err: Error) => {
  cleanup();
  process.stderr.write(`${prefix} ${C.red}FAIL spawn: ${err.message}${C.reset}\n`);
  process.exit(127);
});

child.on("exit", (code: number | null, signal: NodeJS.Signals | null) => {
  cleanup();
  if (killReason) {
    process.stderr.write(`${prefix} ${C.red}FAIL ${killReason} after ${stamp()}${C.reset}\n`);
    process.exit(124);
  }
  if (code === 0) {
    process.stdout.write(`${prefix} ${C.green}OK in ${stamp()}${C.reset}\n`);
    process.exit(0);
  }
  process.stderr.write(
    `${prefix} ${C.red}FAIL exit=${code ?? `signal:${signal}`} in ${stamp()}${C.reset}\n`,
  );
  process.exit(code ?? 1);
});
