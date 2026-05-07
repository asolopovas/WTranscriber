#!/usr/bin/env bun
import { existsSync, statSync, mkdirSync, copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { platform, homedir } from "node:os";

const [, , subcmd, ...rest] = process.argv;
if (!subcmd || !["build", "dev", "doctor", "cli"].includes(subcmd)) {
  console.error("usage: bun scripts/android.mjs <build|dev|doctor|cli> [--target=aarch64] [--debug] [--release] [--open]");
  process.exit(1);
}

const args = new Map();
const flags = new Set();
for (const a of rest) {
  if (a.startsWith("--") && a.includes("=")) {
    const [k, v] = a.slice(2).split("=", 2);
    args.set(k, v);
  } else if (a.startsWith("--")) {
    flags.add(a.slice(2));
  }
}
const target = args.get("target") ?? "aarch64";

const OS = platform();
const isWin = OS === "win32";
const isMac = OS === "darwin";

const root = join(import.meta.dir, "..");
const defaultSdk = isWin
  ? join(process.env.LOCALAPPDATA ?? "", "Android", "Sdk")
  : isMac
    ? join(homedir(), "Library", "Android", "sdk")
    : join(homedir(), "Android", "Sdk");

const ANDROID_HOME = process.env.ANDROID_HOME ?? defaultSdk;
const NDK_HOME = process.env.NDK_HOME ?? join(ANDROID_HOME, "ndk", "27.2.12479018");
const ndkHost = isWin ? "windows-x86_64" : isMac ? "darwin-x86_64" : "linux-x86_64";
const ndkBin = join(NDK_HOME, "toolchains", "llvm", "prebuilt", ndkHost, "bin");
const clangExt = isWin ? ".cmd" : "";
const exeExt = isWin ? ".exe" : "";

const abiMap = {
  aarch64: { abi: "arm64-v8a", rust: "aarch64_linux_android", clang: "aarch64-linux-android24-clang" },
  armv7: { abi: "armeabi-v7a", rust: "armv7_linux_androideabi", clang: "armv7a-linux-androideabi24-clang" },
  i686: { abi: "x86", rust: "i686_linux_android", clang: "i686-linux-android24-clang" },
  x86_64: { abi: "x86_64", rust: "x86_64_linux_android", clang: "x86_64-linux-android24-clang" },
};
const t = abiMap[target];
if (!t) {
  console.error(`unknown target: ${target} (expected: ${Object.keys(abiMap).join(", ")})`);
  process.exit(1);
}

const prebuiltDir = join(root, ".android-prebuilt", "jniLibs", t.abi);
const sherpaLib = join(prebuiltDir, "libsherpa-onnx-c-api.so");

if (subcmd === "doctor") {
  const lines = [
    ["OS", OS],
    ["ANDROID_HOME", ANDROID_HOME],
    ["NDK_HOME", NDK_HOME],
    ["NDK exists", existsSync(NDK_HOME)],
    ["NDK toolchain", ndkBin],
    ["target", target],
    ["abi", t.abi],
    ["sherpa prebuilt", sherpaLib],
    ["sherpa exists", existsSync(sherpaLib)],
  ];
  for (const [k, v] of lines) console.log(`${k.padEnd(18)} ${v}`);
  process.exit(0);
}

if (!existsSync(NDK_HOME)) {
  console.error(`NDK not found: ${NDK_HOME}\nset NDK_HOME or install Android NDK 27.2.12479018`);
  process.exit(1);
}
if (!existsSync(sherpaLib)) {
  if (isWin) {
    console.error("sherpa-onnx prebuilts missing — running install-android-prebuilts.ps1");
    const r = spawnSync("pwsh", ["-NoLogo", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", join(root, "scripts", "install-android-prebuilts.ps1")], { stdio: "inherit" });
    if (r.status !== 0) process.exit(r.status ?? 1);
  } else {
    console.error(`sherpa-onnx prebuilts missing at: ${prebuiltDir}\n(install-android-prebuilts is Windows-only; port to .mjs needed for ${OS})`);
    process.exit(1);
  }
}

if (subcmd === "dev") {
  const dev = spawnSync("adb", ["devices"], { encoding: "utf8" });
  const hasDevice = (dev.stdout ?? "").split("\n").some((l) => /\sdevice$/.test(l.trim()));
  if (!hasDevice) {
    console.error("no adb device — connect device and enable USB debugging");
    process.exit(1);
  }
}

const env = {
  ...process.env,
  ANDROID_HOME,
  NDK_HOME,
  SHERPA_ONNX_LIB_DIR: prebuiltDir,
  [`CC_${t.rust}`]: join(ndkBin, t.clang + clangExt),
  [`CXX_${t.rust}`]: join(ndkBin, t.clang + "++" + clangExt),
  [`AR_${t.rust}`]: join(ndkBin, "llvm-ar" + exeExt),
  [`CARGO_TARGET_${t.rust.toUpperCase()}_LINKER`]: join(ndkBin, t.clang + clangExt),
};

if (subcmd === "cli") {
  const variant = flags.has("debug") ? "debug" : "release";
  const tripleDir = `${target}-linux-android${target === "armv7" ? "eabi" : ""}`;
  const cargoArgs = ["build", "--manifest-path", join(root, "src-tauri", "Cargo.toml"), "--bin", "wt", "--target", tripleDir];
  if (variant === "release") cargoArgs.push("--release");
  const r = spawnSync("cargo", cargoArgs, { cwd: root, env, stdio: "inherit", shell: isWin });
  if (r.status !== 0) process.exit(r.status ?? 1);
  const bin = join(root, "src-tauri", "target", tripleDir, variant, "wt");
  if (existsSync(bin)) {
    const sizeMb = (statSync(bin).size / 1024 / 1024).toFixed(1);
    console.log(`\nbinary: ${bin}\nsize: ${sizeMb} MB`);
  }
  process.exit(0);
}

const llamaSrc = join(root, "src-tauri", "jniLibs", t.abi, "libllama-cli.so");
const genJniDir = join(root, "src-tauri", "gen", "android", "app", "src", "main", "jniLibs", t.abi);
if (existsSync(llamaSrc) && existsSync(genJniDir)) {
  mkdirSync(genJniDir, { recursive: true });
  copyFileSync(llamaSrc, join(genJniDir, "libllama-cli.so"));
}

const manifestPath = join(root, "src-tauri", "gen", "android", "app", "src", "main", "AndroidManifest.xml");
if (existsSync(manifestPath)) {
  let manifest = readFileSync(manifestPath, "utf8");
  if (!manifest.includes("android:extractNativeLibs")) {
    manifest = manifest.replace(
      /(<application\b)/,
      '$1\n        android:extractNativeLibs="true"',
    );
    writeFileSync(manifestPath, manifest);
  }
}

const tauriArgs = ["run", "tauri", "android", subcmd, "--target", target];
if (subcmd === "build") tauriArgs.push("--apk");
if (flags.has("debug")) tauriArgs.push("--debug");
if (flags.has("release")) tauriArgs.push("--release");
if (flags.has("open")) tauriArgs.push("--open");

const result = spawnSync("bun", tauriArgs, { cwd: root, env, stdio: "inherit", shell: isWin });
if (result.status !== 0) process.exit(result.status ?? 1);

if (subcmd === "build") {
  const variant = flags.has("debug") ? "debug" : "release";
  const apkName = variant === "debug" ? "app-universal-debug.apk" : "app-universal-release-unsigned.apk";
  const apk = join(root, "src-tauri", "gen", "android", "app", "build", "outputs", "apk", "universal", variant, apkName);
  if (existsSync(apk)) {
    const sizeMb = (statSync(apk).size / 1024 / 1024).toFixed(1);
    console.log(`\nAPK: ${apk}\nsize: ${sizeMb} MB`);
  }
}
