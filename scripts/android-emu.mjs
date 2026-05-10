#!/usr/bin/env node
// Cross-platform Android emulator boot/stop with bounded waits and heartbeats.
// All operations have an explicit deadline; nothing waits forever.
//
// Usage:
//   bun scripts/android-emu.mjs start [--name wt] [--image system-images;...]
//   bun scripts/android-emu.mjs stop  [--name wt]

import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync, openSync, unlinkSync } from "node:fs";
import os from "node:os";
import path from "node:path";

const isWin = process.platform === "win32";
const sdk =
  process.env.ANDROID_HOME ||
  (isWin
    ? path.join(os.homedir(), "AppData", "Local", "Android", "Sdk")
    : process.platform === "darwin"
      ? path.join(os.homedir(), "Library", "Android", "sdk")
      : path.join(os.homedir(), "Android", "Sdk"));

const exe = (n) => (isWin ? `${n}.exe` : n);
const bat = (n) => (isWin ? `${n}.bat` : n);
const adb = path.join(sdk, "platform-tools", exe("adb"));
const avdmanager = path.join(sdk, "cmdline-tools", "latest", "bin", bat("avdmanager"));
const emulator = path.join(sdk, "emulator", exe("emulator"));

const args = process.argv.slice(2);
const cmd = args[0];
let name = "wt";
let image = "system-images;android-34;google_apis_playstore;x86_64";
for (let i = 1; i < args.length; i++) {
  if (args[i] === "--name") name = args[++i];
  else if (args[i] === "--image") image = args[++i];
}

const tmpDir = path.resolve("tmp");
mkdirSync(tmpDir, { recursive: true });
const pidFile = path.join(tmpDir, "emulator.pid");
const logFile = path.join(tmpDir, "emulator.log");

function log(msg) {
  process.stderr.write(`[emu] ${msg}\n`);
}

function sh(prog, argv, opts = {}) {
  const r = spawnSync(prog, argv, { encoding: "utf8", ...opts });
  return { code: r.status ?? 1, out: (r.stdout || "") + (r.stderr || "") };
}

async function withDeadline(label, deadlineMs, intervalMs, probe) {
  const start = Date.now();
  let lastBeat = start;
  while (Date.now() - start < deadlineMs) {
    const result = await probe();
    if (result) return result;
    if (Date.now() - lastBeat >= 5000) {
      log(`${label}… ${((Date.now() - start) / 1000).toFixed(0)}s elapsed`);
      lastBeat = Date.now();
    }
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new Error(`${label}: timed out after ${(deadlineMs / 1000).toFixed(0)}s`);
}

function adbDevices() {
  const r = sh(adb, ["devices"]);
  if (r.code !== 0) return [];
  return r.out
    .split(/\r?\n/)
    .slice(1)
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith("*"))
    .map((l) => l.split(/\s+/)[0]);
}

async function start() {
  if (!existsSync(adb)) {
    log(`adb not found at ${adb} (set ANDROID_HOME)`);
    process.exit(1);
  }
  if (adbDevices().some((d) => d.startsWith("emulator-"))) {
    log("emulator already running");
    log(adbDevices().join(", "));
    return;
  }
  if (!existsSync(avdmanager) || !existsSync(emulator)) {
    log(`emulator/avdmanager not found in ${sdk} (install via Android Studio)`);
    process.exit(1);
  }
  const list = sh(avdmanager, ["list", "avd"]).out;
  if (!new RegExp(`Name:\\s+${name}\\b`).test(list)) {
    log(`creating AVD '${name}' from ${image}`);
    const create = spawnSync(avdmanager, ["create", "avd", "-n", name, "-k", image, "-d", "pixel_6", "--force"], {
      input: "no\n",
      encoding: "utf8",
    });
    if (create.status !== 0) {
      log(create.stdout + create.stderr);
      process.exit(1);
    }
  }
  log(`booting AVD '${name}' (headless, swiftshader, KVM)`);
  const out = openSync(logFile, "a");
  const child = spawn(
    emulator,
    [
      "-avd",
      name,
      "-no-window",
      "-no-audio",
      "-no-snapshot-save",
      "-gpu",
      "swiftshader_indirect",
      "-accel",
      "on",
      "-netdelay",
      "none",
      "-netspeed",
      "full",
    ],
    { stdio: ["ignore", out, out], detached: true },
  );
  child.unref();
  writeFileSync(pidFile, String(child.pid));

  await withDeadline("waiting for adb device", 60_000, 1000, async () =>
    adbDevices().some((d) => d.startsWith("emulator-")),
  );
  await withDeadline("waiting for sys.boot_completed", 180_000, 2000, async () => {
    const r = sh(adb, ["shell", "getprop", "sys.boot_completed"]);
    return r.code === 0 && r.out.trim() === "1";
  });
  sh(adb, ["shell", "input", "keyevent", "82"]);
  log(`emulator ready: ${adbDevices().join(", ")}`);
}

function stop() {
  const r = sh(adb, ["-s", "emulator-5554", "emu", "kill"]);
  if (r.code === 0) log("sent emu kill");
  if (existsSync(pidFile)) {
    const pid = Number(readFileSync(pidFile, "utf8").trim());
    if (Number.isInteger(pid) && pid > 0) {
      try {
        process.kill(pid);
      } catch {}
    }
    try {
      unlinkSync(pidFile);
    } catch {}
  }
  log("emulator stopped");
}

if (cmd === "start") {
  start().catch((err) => {
    log(`FAIL ${err.message}`);
    process.exit(1);
  });
} else if (cmd === "stop") {
  stop();
} else {
  log("usage: android-emu.mjs start|stop [--name wt] [--image ...]");
  process.exit(2);
}
