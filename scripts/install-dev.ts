#!/usr/bin/env bun
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { homedir, platform } from "node:os";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..");
const BUNDLE_DIR = join(ROOT, "src-tauri", "target", "release", "bundle", "nsis");
const interactive = process.argv.includes("--interactive");

if (platform() !== "win32") {
  console.error("install-dev: Windows host installer only; use `just install` on Linux.");
  process.exit(1);
}

const config = JSON.parse(readFileSync(join(ROOT, "src-tauri", "tauri.conf.json"), "utf8"));
const installer = join(BUNDLE_DIR, `${config.productName}_${config.version}_x64-setup.exe`);
if (!existsSync(installer)) {
  console.error(`install-dev: no installer at ${installer}. Run \`just install\` to build it.`);
  process.exit(1);
}

const args = interactive ? [] : ["/S"];
console.log(`install-dev: running ${installer}${interactive ? "" : " (silent)"}`);
const res = spawnSync(installer, args, { stdio: "inherit" });
if (res.error) {
  console.error(`install-dev: failed to launch installer: ${res.error.message}`);
  process.exit(1);
}
if (res.status !== 0) {
  console.error(`install-dev: installer failed (${res.signal ?? res.status})`);
  process.exit(res.status ?? 1);
}

const installed = join(homedir(), "AppData", "Local", "WTranscriber", "wtranscriber.exe");
console.log(`install-dev: ✓ installed${existsSync(installed) ? ` → ${installed}` : ""}`);
