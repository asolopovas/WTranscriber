#!/usr/bin/env bun
import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { homedir, platform } from "node:os";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..");
const DEV_DIR = join(ROOT, "releases", "dev");
const interactive = process.argv.includes("--interactive");

function newestInstaller(): string | null {
  if (!existsSync(DEV_DIR)) return null;
  const matches = readdirSync(DEV_DIR)
    .filter((n) => /^wtranscriber-setup-.*\.exe$/i.test(n))
    .map((n) => join(DEV_DIR, n))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return matches[0] ?? null;
}

if (platform() !== "win32") {
  console.error(
    "install-dev: Windows host installer only; use `just build` artefacts on other platforms.",
  );
  process.exit(1);
}

const installer = newestInstaller();
if (!installer) {
  console.error(`install-dev: no installer in ${DEV_DIR}. Run \`just build-host\` first.`);
  process.exit(1);
}

const args = interactive ? [] : ["/S"];
console.log(`install-dev: running ${installer}${interactive ? "" : " (silent)"}`);
const res = spawnSync(installer, args, { stdio: "inherit" });
if (res.error) {
  console.error(`install-dev: failed to launch installer: ${res.error.message}`);
  process.exit(1);
}
if (typeof res.status === "number" && res.status !== 0) {
  console.error(`install-dev: installer exited with code ${res.status}`);
  process.exit(res.status);
}

const installed = join(homedir(), "AppData", "Local", "WTranscriber", "wtranscriber.exe");
console.log(`install-dev: ✓ installed${existsSync(installed) ? ` → ${installed}` : ""}`);
