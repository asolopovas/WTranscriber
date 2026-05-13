#!/usr/bin/env bun
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir, platform } from "node:os";
import { join } from "node:path";

const ROOT = join(import.meta.dir, "..");
const PKG = "com.asolopovas.wtranscriber";
const APK = join(
  ROOT,
  "src-tauri",
  "gen",
  "android",
  "app",
  "build",
  "outputs",
  "apk",
  "universal",
  "release",
  "app-universal-release.apk",
);

const force = process.argv.includes("--force");

function androidEnv(): Record<string, string> {
  const home = homedir();
  const sdk =
    process.env.ANDROID_HOME ??
    (platform() === "win32"
      ? join(home, "AppData", "Local", "Android", "Sdk")
      : join(home, "Android", "Sdk"));
  const ndk = process.env.NDK_HOME ?? join(sdk, "ndk", "27.2.12479018");
  const libclang =
    process.env.LIBCLANG_PATH ??
    (platform() === "win32" ? "C:\\Program Files\\LLVM\\bin" : "/usr/lib/x86_64-linux-gnu");
  return {
    ANDROID_HOME: sdk,
    NDK_HOME: ndk,
    ANDROID_NDK: ndk,
    ANDROID_NDK_ROOT: ndk,
    ANDROID_NDK_HOME: ndk,
    LIBCLANG_PATH: libclang,
    CMAKE_GENERATOR: process.env.CMAKE_GENERATOR ?? "Ninja",
    CL: process.env.CL ?? (platform() === "win32" ? "/FS" : ""),
  };
}

function run(cmd: string, args: string[], env?: Record<string, string>): number {
  console.log(`$ ${cmd} ${args.join(" ")}`);
  const r = spawnSync(cmd, args, {
    cwd: ROOT,
    stdio: "inherit",
    env: { ...process.env, ...(env ?? {}) },
  });
  return r.status ?? 1;
}

function adbInstall(): { code: number; sigMismatch: boolean } {
  console.log(`$ adb install -r ${APK}`);
  const r = spawnSync("adb", ["install", "-r", APK], { cwd: ROOT, encoding: "utf8" });
  const out = `${r.stdout ?? ""}${r.stderr ?? ""}`;
  process.stdout.write(out);
  return {
    code: r.status ?? 1,
    sigMismatch: out.includes("INSTALL_FAILED_UPDATE_INCOMPATIBLE"),
  };
}

const buildRc = run("cargo", ["xtask", "android", "build"], androidEnv());
if (buildRc !== 0) process.exit(buildRc);
if (!existsSync(APK)) {
  console.error(`APK not found at ${APK}`);
  process.exit(1);
}

let res = adbInstall();
if (res.sigMismatch) {
  if (!force) {
    console.error(
      `\nSignature mismatch — the installed app was signed with a different keystore.\nRe-run with --force to uninstall (wipes app data) and reinstall.`,
    );
    process.exit(1);
  }
  console.log(`\nSignature mismatch; uninstalling ${PKG} (wipes data)...`);
  const un = run("adb", ["uninstall", PKG]);
  if (un !== 0) process.exit(un);
  res = adbInstall();
}
process.exit(res.code);
