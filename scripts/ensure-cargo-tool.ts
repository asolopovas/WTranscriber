#!/usr/bin/env bun
import { spawnSync } from "node:child_process";

const tool = process.argv[2];
if (!tool) {
  console.error("ensure-cargo-tool: tool name required");
  process.exit(2);
}

const probe = spawnSync(tool, ["--version"], { stdio: "ignore", shell: false });
if (probe.status === 0) process.exit(0);

const r = spawnSync(
  "bun",
  [
    "scripts/run.ts",
    "--tag",
    `install-${tool}`,
    "--idle",
    "60",
    "--max",
    "600",
    "--",
    "cargo",
    "install",
    "--locked",
    tool,
  ],
  { stdio: "inherit", shell: false },
);
process.exit(r.status ?? 1);
