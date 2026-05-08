#!/usr/bin/env node
import { chromium } from "playwright";
import { spawn } from "node:child_process";
import { appendFileSync, mkdirSync } from "node:fs";

const LOG_FILE = "tmp/error-monitor.log";
mkdirSync("tmp", { recursive: true });

function emit(source, level, message) {
  const ts = new Date().toISOString().slice(11, 23);
  const line = `[${ts}] ${source}/${level}: ${message}`;
  console.log(line);
  appendFileSync(LOG_FILE, line + "\n");
}

const NOISE = [
  /reqwest::connect/,
  /hyper_util::client::legacy::connect/,
  /HwcComposer/,
  /SurfaceFlinger/,
  /SemGameManager/,
  /SGM:/,
  /C2BqBuffer/,
  /setRequestedFrameRate/,
  /RenderEngine/,
  /BufferQueueProducer/,
  /ViewRootImpl/,
  /InsetsController/,
  /chatty.*identical/,
];
function noisy(s) {
  return NOISE.some((re) => re.test(s));
}

const recent = new Map();
function dedupe(key) {
  const now = Date.now();
  const last = recent.get(key) ?? 0;
  recent.set(key, now);
  if (now - last < 2000) return true;
  if (recent.size > 500) {
    const cutoff = now - 30000;
    for (const [k, v] of recent) if (v < cutoff) recent.delete(k);
  }
  return false;
}

const logcat = spawn("adb", ["logcat", "-T", "1", "-v", "time", "*:W"], {
  stdio: ["ignore", "pipe", "pipe"],
});
let buf = "";
logcat.stdout.on("data", (chunk) => {
  buf += chunk.toString();
  const lines = buf.split("\n");
  buf = lines.pop() ?? "";
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (!line || noisy(line)) continue;
    const m = line.match(/^\d\d-\d\d \d\d:\d\d:\d\d\.\d+\s+\d+\s+\d+\s+([VDIWEF])\s+([^:]+):\s*(.*)$/);
    if (!m) continue;
    const [, lvl, tag, msg] = m;
    if (lvl !== "E" && lvl !== "F" && !/RustStdoutStderr/.test(tag) && !/Console|chromium/.test(tag))
      continue;
    if (/RustStdoutStderr/.test(tag)) {
      if (!/\bERROR\b|\bWARN\b|panicked|thread\s'\w+'/i.test(msg)) continue;
    }
    if (dedupe(`${tag}:${msg.slice(0, 80)}`)) continue;
    emit("logcat", lvl, `${tag}: ${msg.slice(0, 280)}`);
  }
});
logcat.stderr.on("data", (d) => emit("logcat", "stderr", d.toString().trim()));
logcat.on("exit", (code) => emit("logcat", "exit", `code=${code}`));

let cdp;
async function attachCDP() {
  for (let i = 0; i < 60; i++) {
    try {
      cdp = await chromium.connectOverCDP("http://localhost:9222");
      break;
    } catch {
      await new Promise((r) => setTimeout(r, 2000));
    }
  }
  if (!cdp) {
    emit("cdp", "ERROR", "could not connect to webview devtools at :9222");
    return;
  }
  const ctx = cdp.contexts()[0];
  const wirePage = (page) => {
    page.on("console", (msg) => {
      const t = msg.type();
      if (t !== "error" && t !== "warning" && t !== "warn") return;
      const text = msg.text();
      if (dedupe(`console:${text.slice(0, 80)}`)) return;
      emit("console", t, text.slice(0, 500));
    });
    page.on("pageerror", (err) => {
      emit("pageerror", "ERROR", `${err.name}: ${err.message}`);
    });
    page.on("requestfailed", (req) => {
      const f = req.failure();
      if (!f) return;
      emit("net", "fail", `${req.method()} ${req.url()} \u2014 ${f.errorText}`);
    });
  };
  for (const page of ctx.pages()) wirePage(page);
  ctx.on("page", wirePage);
  emit("cdp", "info", "attached to webview");
}
attachCDP();

emit("monitor", "info", "up");

process.on("SIGINT", () => process.exit(0));
process.on("SIGTERM", () => process.exit(0));
