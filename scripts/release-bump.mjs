#!/usr/bin/env bun
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";

const root = join(import.meta.dir, "..");
const pkgPath = join(root, "package.json");
const cargoPath = join(root, "src-tauri", "Cargo.toml");
const tauriPath = join(root, "src-tauri", "tauri.conf.json");

const arg = process.argv[2];

function sh(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { cwd: root, stdio: "pipe", encoding: "utf8", ...opts });
  if (r.status !== 0) {
    process.stderr.write(r.stderr || "");
    process.exit(r.status ?? 1);
  }
  return (r.stdout || "").trim();
}

function fail(msg) {
  console.error(`ERROR: ${msg}`);
  process.exit(1);
}

function readVersion() {
  const pkg = JSON.parse(readFileSync(pkgPath, "utf8"));
  return pkg.version;
}

function bumpPatch(v) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(v);
  if (!m) fail(`current version "${v}" is not strict X.Y.Z SemVer`);
  return `${m[1]}.${m[2]}.${Number(m[3]) + 1}`;
}

function setPkg(ver) {
  const raw = readFileSync(pkgPath, "utf8");
  const pkg = JSON.parse(raw);
  pkg.version = ver;
  writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + "\n");
}

function setCargo(ver) {
  const raw = readFileSync(cargoPath, "utf8");
  const next = raw.replace(/^version\s*=\s*"[^"]+"/m, `version = "${ver}"`);
  if (next === raw) fail("could not find version line in src-tauri/Cargo.toml");
  writeFileSync(cargoPath, next);
}

function setTauri(ver) {
  const raw = readFileSync(tauriPath, "utf8");
  const cfg = JSON.parse(raw);
  cfg.version = ver;
  writeFileSync(tauriPath, JSON.stringify(cfg, null, 2) + "\n");
}

function syncCargoLock() {
  const lockPath = join(root, "src-tauri", "Cargo.lock");
  if (!existsSync(lockPath)) return false;
  const r = spawnSync("cargo", ["update", "--manifest-path", "src-tauri/Cargo.toml", "-w", "--offline"], {
    cwd: root,
    stdio: "inherit",
    shell: process.platform === "win32",
  });
  if (r.status !== 0) fail(`cargo update failed (exit ${r.status})`);
  return true;
}

function ensureClean() {
  const dirty = sh("git", ["status", "--porcelain"]);
  if (dirty) fail("working tree is dirty; commit or stash first");
}

function tagExists(tag) {
  const r = spawnSync("git", ["rev-parse", tag], { cwd: root, stdio: "pipe" });
  return r.status === 0;
}

if (!arg || arg === "--help" || arg === "-h") {
  console.log("usage: bun scripts/release-bump.mjs <patch | minor | major | X.Y.Z>");
  process.exit(arg ? 0 : 1);
}

ensureClean();
const cur = readVersion();
let next;
if (arg === "patch") next = bumpPatch(cur);
else if (arg === "minor") {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(cur);
  if (!m) fail(`current version "${cur}" is not X.Y.Z`);
  next = `${m[1]}.${Number(m[2]) + 1}.0`;
} else if (arg === "major") {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(cur);
  if (!m) fail(`current version "${cur}" is not X.Y.Z`);
  next = `${Number(m[1]) + 1}.0.0`;
} else if (/^\d+\.\d+\.\d+$/.test(arg)) {
  next = arg;
} else {
  fail(`invalid argument "${arg}" — expected patch|minor|major|X.Y.Z`);
}

const tag = `v${next}`;
if (tagExists(tag)) fail(`tag ${tag} already exists`);

console.log(`bump: ${cur} -> ${next}`);
setPkg(next);
setCargo(next);
setTauri(next);
const haveLock = syncCargoLock();

const toAdd = ["package.json", "src-tauri/Cargo.toml", "src-tauri/tauri.conf.json"];
if (haveLock) toAdd.push("src-tauri/Cargo.lock");
sh("git", ["add", ...toAdd]);
sh("git", ["commit", "--no-verify", "-m", `chore(release): ${next}`]);
sh("git", ["tag", "-a", tag, "-m", `Release ${tag}`]);
console.log(`tagged ${tag}`);
