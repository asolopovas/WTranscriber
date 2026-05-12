#!/usr/bin/env bun
import { spawn, spawnSync, type SpawnSyncOptionsWithStringEncoding } from "node:child_process";
import { mkdirSync, openSync, unlinkSync } from "node:fs";
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

const exe = (n: string): string => (isWin ? `${n}.exe` : n);
const bat = (n: string): string => (isWin ? `${n}.bat` : n);
const adb = path.join(sdk, "platform-tools", exe("adb"));
const avdmanager = path.join(sdk, "cmdline-tools", "latest", "bin", bat("avdmanager"));
const emulator = path.join(sdk, "emulator", exe("emulator"));

const args = process.argv.slice(2);
const cmd = args[0];
let name = "wt";
let image = "system-images;android-34;google_apis_playstore;x86_64";
let windowed = false;
for (let i = 1; i < args.length; i++) {
  if (args[i] === "--name") name = args[++i] ?? name;
  else if (args[i] === "--image") image = args[++i] ?? image;
  else if (args[i] === "--window") windowed = true;
}

const tmpDir = path.resolve("tmp");
mkdirSync(tmpDir, { recursive: true });
const pidFile = path.join(tmpDir, "emulator.pid");
const logFile = path.join(tmpDir, "emulator.log");

const log = (msg: string): void => {
  process.stderr.write(`[emu] ${msg}\n`);
};

interface ShResult {
  code: number;
  out: string;
}

const sh = (
  prog: string,
  argv: string[],
  opts: Partial<SpawnSyncOptionsWithStringEncoding> = {},
): ShResult => {
  const r = spawnSync(prog, argv, { encoding: "utf8", ...opts });
  return { code: r.status ?? 1, out: (r.stdout || "") + (r.stderr || "") };
};

const fileExists = async (p: string): Promise<boolean> => Bun.file(p).exists();

const withDeadline = async <T>(
  label: string,
  deadlineMs: number,
  intervalMs: number,
  probe: () => Promise<T | false> | T | false,
): Promise<T> => {
  const start = Date.now();
  let lastBeat = start;
  while (Date.now() - start < deadlineMs) {
    const result = await probe();
    if (result) return result as T;
    if (Date.now() - lastBeat >= 5000) {
      log(`${label}… ${((Date.now() - start) / 1000).toFixed(0)}s elapsed`);
      lastBeat = Date.now();
    }
    await new Promise((r) => setTimeout(r, intervalMs));
  }
  throw new Error(`${label}: timed out after ${(deadlineMs / 1000).toFixed(0)}s`);
};

const adbDevices = (): string[] => {
  const r = sh(adb, ["devices"]);
  if (r.code !== 0) return [];
  return r.out
    .split(/\r?\n/)
    .slice(1)
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith("*"))
    .map((l) => l.split(/\s+/)[0]!)
    .filter((s): s is string => Boolean(s));
};

const start = async (): Promise<void> => {
  if (!(await fileExists(adb))) {
    log(`adb not found at ${adb} (set ANDROID_HOME)`);
    process.exit(1);
  }
  if (adbDevices().some((d) => d.startsWith("emulator-"))) {
    log("emulator already running");
    log(adbDevices().join(", "));
    return;
  }
  if (!(await fileExists(avdmanager)) || !(await fileExists(emulator))) {
    log(`emulator/avdmanager not found in ${sdk} (install via Android Studio)`);
    process.exit(1);
  }
  const list = sh(avdmanager, ["list", "avd"]).out;
  if (!new RegExp(`Name:\\s+${name}\\b`).test(list)) {
    log(`creating AVD '${name}' from ${image}`);
    const create = spawnSync(
      avdmanager,
      ["create", "avd", "-n", name, "-k", image, "-d", "pixel_6", "--force"],
      { input: "no\n", encoding: "utf8" },
    );
    if (create.status !== 0) {
      log((create.stdout ?? "") + (create.stderr ?? ""));
      process.exit(1);
    }
  }
  log(
    `booting AVD '${name}' (${windowed ? "windowed" : "headless"}, ${windowed ? "host gpu" : "swiftshader"}, KVM)`,
  );
  const out = openSync(logFile, "a");
  const emuArgs = [
    "-avd",
    name,
    "-no-snapshot-save",
    "-accel",
    "on",
    "-netdelay",
    "none",
    "-netspeed",
    "full",
  ];
  if (windowed) {
    emuArgs.push("-gpu", "auto");
  } else {
    emuArgs.push("-no-window", "-no-audio", "-gpu", "swiftshader_indirect");
  }
  const child = spawn(emulator, emuArgs, { stdio: ["ignore", out, out], detached: true });
  child.unref();
  await Bun.write(pidFile, String(child.pid));

  await withDeadline("waiting for adb device", 60_000, 1000, () =>
    adbDevices().some((d) => d.startsWith("emulator-")),
  );
  await withDeadline("waiting for sys.boot_completed", 180_000, 2000, () => {
    const r = sh(adb, ["shell", "getprop", "sys.boot_completed"]);
    return r.code === 0 && r.out.trim() === "1";
  });
  sh(adb, ["shell", "input", "keyevent", "82"]);
  log(`emulator ready: ${adbDevices().join(", ")}`);
};

const stop = async (): Promise<void> => {
  const r = sh(adb, ["-s", "emulator-5554", "emu", "kill"]);
  if (r.code === 0) log("sent emu kill");
  const pidF = Bun.file(pidFile);
  if (await pidF.exists()) {
    const pid = Number((await pidF.text()).trim());
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
};

if (cmd === "start") {
  start().catch((err: Error) => {
    log(`FAIL ${err.message}`);
    process.exit(1);
  });
} else if (cmd === "stop") {
  await stop();
} else {
  log("usage: android-emu.ts start|stop [--name wt] [--image ...]");
  process.exit(2);
}
