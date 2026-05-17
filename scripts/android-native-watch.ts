#!/usr/bin/env bun

import { existsSync, readdirSync, statSync, openSync, closeSync } from "node:fs";
import { join } from "node:path";

const mode = process.argv[2] ?? "usb";
const device = process.argv[3] ?? "";
const root = process.cwd();
const tmp = join(root, "tmp");
const lockPath = join(tmp, "android-native-reload.lock");

const roots = [
  "src-tauri/src",
  "src-tauri/capabilities",
  "src-tauri/Cargo.toml",
  "src-tauri/Cargo.lock",
  "src-tauri/build.rs",
  "src-tauri/tauri.conf.json",
];

const ignoredDirs = new Set(["target", "gen", ".git", "node_modules", "tmp"]);

function files(path: string): string[] {
  if (!existsSync(path)) return [];
  const st = statSync(path);
  if (st.isFile()) return [path];
  if (!st.isDirectory()) return [];
  const out: string[] = [];
  for (const entry of readdirSync(path, { withFileTypes: true })) {
    if (entry.isDirectory() && ignoredDirs.has(entry.name)) continue;
    const child = join(path, entry.name);
    if (entry.isDirectory()) out.push(...files(child));
    else if (entry.isFile()) out.push(child);
  }
  return out;
}

function snapshot(): Map<string, number> {
  const map = new Map<string, number>();
  for (const rel of roots) {
    for (const file of files(join(root, rel))) {
      map.set(file, statSync(file).mtimeMs);
    }
  }
  return map;
}

let last = snapshot();
let reloadTimer: Timer | null = null;

function changed(): boolean {
  const next = snapshot();
  let dirty = next.size !== last.size;
  if (!dirty) {
    for (const [file, mtime] of next) {
      if (last.get(file) !== mtime) {
        dirty = true;
        break;
      }
    }
  }
  last = next;
  return dirty;
}

function scheduleReload() {
  if (reloadTimer) clearTimeout(reloadTimer);
  reloadTimer = setTimeout(startReload, 1200);
}

function startReload() {
  if (existsSync(lockPath)) return;
  const fd = openSync(lockPath, "w");
  closeSync(fd);
  const args = [
    "run",
    "--quiet",
    "--manifest-path",
    "xtask/Cargo.toml",
    "--target-dir",
    "tmp/xtask-android-bootstrap-target",
    "--",
    "android",
    "bootstrap",
    mode,
  ];
  if (device) args.push(device);
  const out = openSync(join(tmp, "android-reload.log"), "a");
  const err = openSync(join(tmp, "android-reload.err.log"), "a");
  const child = Bun.spawn(["cargo", ...args], {
    cwd: root,
    stdout: out,
    stderr: err,
    stdin: "ignore",
  });
  child.unref();
  process.exit(0);
}

setInterval(() => {
  if (changed()) scheduleReload();
}, 1000);
