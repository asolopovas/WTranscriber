#!/usr/bin/env bun
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";

const args = process.argv.slice(2);
let base = process.env.CHECK_CHANGED_BASE ?? "";
let staged = false;
let all = false;

for (let i = 0; i < args.length; i++) {
  const arg = args[i]!;
  if (arg === "--base") base = args[++i] ?? "";
  else if (arg === "--staged") staged = true;
  else if (arg === "--all") all = true;
  else {
    console.error(`unknown arg ${arg}`);
    process.exit(2);
  }
}

const run = (name: string, cmd: string, cmdArgs: string[]): void => {
  console.log(`→ ${name}: ${[cmd, ...cmdArgs].join(" ")}`);
  const result = spawnSync(cmd, cmdArgs, { stdio: "inherit" });
  if (result.status !== 0) process.exit(result.status ?? 1);
};

const output = (cmd: string, cmdArgs: string[]): string => {
  const result = spawnSync(cmd, cmdArgs, { encoding: "utf8" });
  if (result.status !== 0) return "";
  return result.stdout.trim();
};

const commitExists = (ref: string): boolean => {
  if (!ref || /^0+$/.test(ref)) return false;
  return spawnSync("git", ["cat-file", "-e", `${ref}^{commit}`], { stdio: "ignore" }).status === 0;
};

const changedFiles = (): string[] => {
  if (all) return output("git", ["ls-files"]).split(/\r?\n/).filter(Boolean);
  if (staged)
    return output("git", ["diff", "--cached", "--name-only", "--diff-filter=ACMR"])
      .split(/\r?\n/)
      .filter(Boolean);
  if (commitExists(base)) {
    return output("git", ["diff", "--name-only", "--diff-filter=ACMR", `${base}...HEAD`])
      .split(/\r?\n/)
      .filter(Boolean);
  }
  if (commitExists("HEAD~1")) {
    return output("git", ["diff", "--name-only", "--diff-filter=ACMR", "HEAD~1...HEAD"])
      .split(/\r?\n/)
      .filter(Boolean);
  }
  return output("git", ["ls-files"]).split(/\r?\n/).filter(Boolean);
};

const files = changedFiles().filter((file) => existsSync(file));
if (files.length === 0) {
  console.log("→ no changed files to check");
  process.exit(0);
}

console.log(`→ changed files (${files.length})`);
for (const file of files) console.log(`  ${file}`);

const matches = (re: RegExp): string[] => files.filter((file) => re.test(file));
const has = (re: RegExp): boolean => files.some((file) => re.test(file));
const prettierFiles = files.filter(
  (file) =>
    /^(src|scripts)\/.*\.(ts|vue)$/.test(file) ||
    /\.(json|jsonc|html|md|css|scss|yml|yaml)$/.test(file) ||
    /^(dev|vite|vitest|playwright|tailwind)\.config\.ts$/.test(file),
);
const tauriRust = matches(/^src-tauri\/.*\.rs$/);
const xtaskRust = matches(/^xtask\/.*\.rs$/);
const vueFiles = matches(/^src\/.*\.vue$/);
const needsTypecheck =
  has(/^(src|scripts)\/.*\.(ts|vue)$/) ||
  has(
    /^(package\.json|bun\.lock|tsconfig.*\.json|vite\.config\.ts|vitest\.config\.ts|dev\.config\.ts)$/,
  );
const needsJsTest =
  has(/^(src|scripts)\/.*\.(ts|vue)$/) || has(/^(package\.json|bun\.lock|vitest\.config\.ts)$/);
const needsXtaskCompile = xtaskRust.length > 0 || has(/^xtask\/(Cargo\.toml|Cargo\.lock)$/);
const needsCargoAudit = has(/^src-tauri\/Cargo\.lock$/);
const needsBunAudit = has(/^(package\.json|bun\.lock)$/);

if (prettierFiles.length > 0)
  run("prettier changed files", "bun", ["x", "prettier", "--check", ...prettierFiles]);
if (tauriRust.length > 0)
  run("rustfmt src-tauri changed files", "cargo", [
    "fmt",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--",
    "--check",
    ...tauriRust,
  ]);
if (xtaskRust.length > 0)
  run("rustfmt xtask changed files", "cargo", [
    "fmt",
    "--manifest-path",
    "xtask/Cargo.toml",
    "--",
    "--check",
    ...xtaskRust,
  ]);
if (needsTypecheck) run("typecheck", "bun", ["run", "typecheck"]);
if (vueFiles.length > 0)
  run("lint-vue changed files", "bun", ["run", "scripts/lint-vue.ts", ...vueFiles]);
if (needsJsTest) run("js tests", "bun", ["run", "test"]);
if (needsXtaskCompile) {
  run("xtask clippy", "cargo", [
    "clippy",
    "--manifest-path",
    "xtask/Cargo.toml",
    "--target-dir",
    "tmp/xtask-check-target",
    "--all-targets",
    "--",
    "-D",
    "warnings",
  ]);
  run("xtask tests", "cargo", [
    "test",
    "--manifest-path",
    "xtask/Cargo.toml",
    "--target-dir",
    "tmp/xtask-check-target",
  ]);
}
if (needsCargoAudit) {
  if (spawnSync("cargo-audit", ["--version"], { stdio: "ignore" }).status !== 0) {
    run("install cargo-audit", "cargo", ["install", "--locked", "cargo-audit"]);
  }
  run("cargo audit", "cargo", ["audit", "--file", "src-tauri/Cargo.lock"]);
}
if (needsBunAudit) run("bun audit", "bun", ["audit"]);

console.log("✓ changed-file checks passed");
