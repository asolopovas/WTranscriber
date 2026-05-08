#!/usr/bin/env node
/**
 * Deterministic observer for WTranscriber dev sessions.
 *
 * Tails tmp/error-monitor.log, tmp/android-dev.log, tmp/android-dev.err.log,
 * and a long-running `adb logcat -v threadtime *:S RustStdoutStderr:V chromium:V *:E`.
 * Classifies lines via regex into severity/category buckets and appends to
 * tmp/observer-alerts.md while overwriting tmp/observer-latest.json.
 *
 * Stops when tmp/observer-stop appears, on SIGINT, or on SIGTERM.
 *
 * CDP :9222 tailing is intentionally omitted: a stdlib-only websocket client
 * is not trivially small. Console errors are still surfaced via chromium tag
 * in logcat and via the existing error-monitor's CDP wiring.
 */

import { spawn, spawnSync } from "node:child_process";
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  openSync,
  readSync,
  closeSync,
  statSync,
  writeFileSync,
  readFileSync,
} from "node:fs";
import { createHash } from "node:crypto";

const ROOT = process.cwd();
const ALERTS = "tmp/observer-alerts.md";
const LATEST = "tmp/observer-latest.json";
const STOP = "tmp/observer-stop";
const FILE_SOURCES = [
  { path: "tmp/error-monitor.log", source: "error-monitor" },
  { path: "tmp/android-dev.log", source: "android-dev" },
  { path: "tmp/android-dev.err.log", source: "android-dev.err" },
];
const POLL_MS = 2000;
const STARTED_AT = new Date().toISOString();
const POST_LAUNCH_GRACE_MS = 5000;
const LAUNCH_T0 = Date.now();
const APP_PACKAGE = "com.asolopovas.wtranscriber";
const PID_RETRY_MS = 5000;

mkdirSync("tmp", { recursive: true });

function nowIso() {
  return new Date().toISOString();
}

function countExistingAlerts() {
  if (!existsSync(ALERTS)) return 0;
  try {
    const text = readFileSync(ALERTS, "utf8");
    let n = 0;
    for (const line of text.split("\n")) {
      if (/^## .* \[(critical|warning)\] /.test(line)) n += 1;
    }
    return n;
  } catch {
    return 0;
  }
}

let alertCountTotal = countExistingAlerts();
let alertCountSinceStart = 0;
let lastAlertId = "none";
let lastAlertCategory = "none";
let lastAlertSeverity = "info";
let lastAlertTs = STARTED_AT;

function refreshLatest() {
  const payload = {
    ts: lastAlertTs,
    severity: lastAlertSeverity,
    category: lastAlertCategory,
    alert_count_total: alertCountTotal,
    alert_count_since_start: alertCountSinceStart,
    last_alert_id: lastAlertId,
    pid: process.pid,
    started_at: STARTED_AT,
  };
  writeFileSync(LATEST, JSON.stringify(payload, null, 2) + "\n");
}

function appendAlert(severity, source, category, trigger) {
  const ts = nowIso();
  const oneLine = String(trigger).replace(/\s+/g, " ").trim().slice(0, 200);
  const block = `\n## ${ts} [${severity}] [${source}] ${category}\n${oneLine}\n`;
  appendFileSync(ALERTS, block);
  if (severity === "critical" || severity === "warning") {
    alertCountTotal += 1;
    alertCountSinceStart += 1;
  }
  lastAlertId = createHash("sha1").update(oneLine).digest("hex").slice(0, 16);
  lastAlertCategory = category;
  lastAlertSeverity = severity;
  lastAlertTs = ts;
  refreshLatest();
}

const NOISE = [
  /hyper::client::connect/,
  /hyper_util::client::legacy::connect/,
  /reqwest::connect/,
  /HwcComposer/,
  /SurfaceFlinger/,
  /SemGameManager/,
  /setRequestedFrameRate/,
  /Replacing devUrl host with 127\.0\.0\.1/,
  /\[vite\] hot updated/,
  /\[vite\] page reload/,
];

const THIRD_PARTY = [
  /\bHelium[A-Za-z]*\b/,
  /\bCrashpad\b/,
  /\bcom\.google\./,
  /\bcom\.android\./,
  /\bcom\.samsung\./,
  /\bcom\.instagram\./,
  /\bcom\.facebook\./,
  /\bcom\.sec\./,
];

function isThirdParty(line) {
  return THIRD_PARTY.some((re) => re.test(line));
}

function isNoise(line) {
  return NOISE.some((re) => re.test(line));
}

const RULES = [
  { category: "panic", severity: "critical", re: /RustStdoutStderr.*panic|FATAL EXCEPTION/i },
  {
    category: "webview-crash",
    severity: "critical",
    re: /(?:chromium|WebViewChromium|RustWebView|RustStdoutStderr)\b[^\n]*(?:FATAL|crash(?:ed|ing)?)/i,
  },
  {
    category: "port-lost",
    severity: "critical",
    re: /Failed to request http:\/\/127\.0\.0\.1:1420|EADDRINUSE/,
  },
  {
    category: "ipc-error",
    severity: "warning",
    re: /tauri::error|Tauri command .* failed|invoke .* rejected/i,
  },
  { category: "hmr-broken", severity: "warning", re: /:1421 failed|\[vite\][^\n]*error/i },
  {
    category: "network-fail",
    severity: "warning",
    re: /error sending request for url/i,
    postLaunchOnly: true,
  },
];

const recent = new Map();
function dedupe(key) {
  const now = Date.now();
  const last = recent.get(key) ?? 0;
  recent.set(key, now);
  if (now - last < 5000) return true;
  if (recent.size > 1000) {
    const cutoff = now - 60000;
    for (const [k, v] of recent) if (v < cutoff) recent.delete(k);
  }
  return false;
}

function classify(line, source) {
  if (!line || isNoise(line)) return;
  if (source === "logcat" && isThirdParty(line)) return;
  for (const rule of RULES) {
    if (!rule.re.test(line)) continue;
    if (rule.postLaunchOnly && Date.now() - LAUNCH_T0 < POST_LAUNCH_GRACE_MS) return;
    const key = `${rule.category}:${line.slice(0, 120)}`;
    if (dedupe(key)) return;
    appendAlert(rule.severity, source, rule.category, line);
    return;
  }
}

const tailState = new Map();
for (const { path } of FILE_SOURCES) tailState.set(path, 0);

function pollFile(entry) {
  const { path, source } = entry;
  if (!existsSync(path)) return;
  let st;
  try {
    st = statSync(path);
  } catch {
    return;
  }
  let offset = tailState.get(path) ?? 0;
  if (st.size < offset) offset = 0;
  if (st.size === offset) return;
  const len = st.size - offset;
  const buf = Buffer.alloc(len);
  let fd;
  try {
    fd = openSync(path, "r");
    readSync(fd, buf, 0, len, offset);
  } catch {
    if (fd !== undefined) closeSync(fd);
    return;
  }
  closeSync(fd);
  tailState.set(path, st.size);
  const carry = (tailState.get(`${path}::carry`) ?? "") + buf.toString("utf8");
  const lines = carry.split("\n");
  const tail = lines.pop() ?? "";
  tailState.set(`${path}::carry`, tail);
  for (const raw of lines) {
    const line = raw.trimEnd();
    if (line) classify(line, source);
  }
}

let logcatChild = null;
let logcatBuf = "";
let logcatRespawnTimer = null;
let logcatPid = null;

function resolveAppPid() {
  try {
    const r = spawnSync("adb", ["shell", "pidof", "-s", APP_PACKAGE], {
      encoding: "utf8",
      timeout: 5000,
    });
    if (r.status !== 0) return null;
    const out = (r.stdout ?? "").trim();
    if (!out) return null;
    const pid = parseInt(out.split(/\s+/)[0], 10);
    return Number.isFinite(pid) && pid > 0 ? pid : null;
  } catch {
    return null;
  }
}

function scheduleLogcatRetry() {
  if (stopping || logcatRespawnTimer) return;
  logcatRespawnTimer = setTimeout(() => {
    logcatRespawnTimer = null;
    if (!stopping) spawnLogcat();
  }, PID_RETRY_MS);
}

function spawnLogcat() {
  const pid = resolveAppPid();
  if (pid === null) {
    logcatPid = null;
    scheduleLogcatRetry();
    return;
  }
  logcatPid = pid;
  const args = [
    "logcat",
    "-T",
    "1",
    `--pid=${pid}`,
    "-v",
    "threadtime",
    "*:S",
    "RustStdoutStderr:V",
    "RustWebView:V",
    "WebViewChromium:V",
    "chromium:V",
    "*:E",
  ];
  const child = spawn("adb", args, { stdio: ["ignore", "pipe", "pipe"] });
  logcatChild = child;
  logcatBuf = "";
  child.stdout.on("data", (chunk) => {
    logcatBuf += chunk.toString("utf8");
    const lines = logcatBuf.split("\n");
    logcatBuf = lines.pop() ?? "";
    for (const raw of lines) {
      const line = raw.trimEnd();
      if (line) classify(line, "logcat");
    }
  });
  child.stderr.on("data", () => {});
  child.on("exit", (code) => {
    if (stopping) return;
    logcatChild = null;
    logcatPid = null;
    logcatRespawnTimer = setTimeout(() => {
      logcatRespawnTimer = null;
      if (!stopping) spawnLogcat();
    }, 2000);
  });
  child.on("error", () => {});
}

let stopping = false;
let pollTimer = null;

function shutdown(reason) {
  if (stopping) return;
  stopping = true;
  if (pollTimer) clearInterval(pollTimer);
  if (logcatRespawnTimer) clearTimeout(logcatRespawnTimer);
  if (logcatChild) {
    try {
      logcatChild.kill();
    } catch {}
  }
  const ts = nowIso();
  lastAlertTs = ts;
  lastAlertSeverity = "info";
  lastAlertCategory = "session-end";
  appendFileSync(ALERTS, `\n## ${ts} [info] [observer] session-end\n${reason}\n`);
  refreshLatest();
  process.exit(0);
}

const startTs = nowIso();
lastAlertTs = startTs;
lastAlertSeverity = "info";
lastAlertCategory = "session-start";
appendFileSync(
  ALERTS,
  `\n## ${startTs} [info] [observer] session-start\nSources: ${FILE_SOURCES.map((f) => f.path).join(" \u00b7 ")} \u00b7 adb logcat (RustStdoutStderr/chromium/*:E)\n`,
);
refreshLatest();

spawnLogcat();

pollTimer = setInterval(() => {
  if (existsSync(STOP)) {
    shutdown(`Stop file detected: ${STOP}`);
    return;
  }
  for (const entry of FILE_SOURCES) pollFile(entry);
}, POLL_MS);

process.on("SIGINT", () => shutdown("SIGINT"));
process.on("SIGTERM", () => shutdown("SIGTERM"));
