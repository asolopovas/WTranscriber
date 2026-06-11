#!/usr/bin/env bun
import { spawnSync } from "node:child_process";

const usage = `usage:
  just release
  just release --stable
  just release --bump [patch|minor|major|X.Y.Z]
  just release --stable [--no-android] [--no-deb] [--no-windows-vm] [--skip-rebuild]

Default publishes releases/dev/* to the rolling dev prerelease.
--stable runs the stable patch release flow.
--bump implies --stable and selects the stable version bump.
Matrix flags are forwarded to cargo xtask release-stable, which preflights
gh auth, upstream sync, Android signing, and Docker before bumping anything.`;

const rawArgs = process.argv.slice(2);

if (rawArgs.some((arg) => arg === "--help" || arg === "-h")) {
  console.log(usage);
  process.exit(0);
}

let stable = false;
let bump = false;
const releaseStableArgs: string[] = [];

for (const arg of rawArgs) {
  if (arg === "--stable") {
    stable = true;
    continue;
  }
  if (arg === "--bump" || arg.startsWith("--bump=")) {
    stable = true;
    bump = true;
  }
  releaseStableArgs.push(arg);
}

if (stable && !bump) {
  releaseStableArgs.push("--bump", "patch");
}

if (!stable && rawArgs.length > 0) {
  console.error(`release: unknown dev release argument(s): ${rawArgs.join(" ")}`);
  console.error(usage);
  process.exit(2);
}

const command = stable
  ? ["cargo", "xtask", "release-stable", ...releaseStableArgs]
  : ["cargo", "xtask", "publish", "dev"];

console.log(`release: ${command.join(" ")}`);
const result = spawnSync(command[0]!, command.slice(1), { stdio: "inherit" });

if (result.error) {
  console.error(`release: failed to spawn ${command[0]}: ${result.error.message}`);
  process.exit(127);
}

if (result.signal) {
  console.error(`release: ${command[0]} exited with signal ${result.signal}`);
  process.exit(1);
}

process.exit(result.status ?? 1);
