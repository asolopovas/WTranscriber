#!/usr/bin/env bun
import { existsSync, mkdirSync, copyFileSync, readFileSync, writeFileSync, statSync, readdirSync, rmSync } from "node:fs";
import { join, basename } from "node:path";
import { spawnSync, execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { platform } from "node:os";

const root = join(import.meta.dir, "..");
const isWin = platform() === "win32";

const args = new Set(process.argv.slice(2));
const dev = args.has("--dev");
const skipAndroid = args.has("--no-android");
const skipHost = args.has("--no-host");
const skipRebuild = args.has("--skip-rebuild");

function sh(cmd, params, opts = {}) {
  const r = spawnSync(cmd, params, { cwd: root, stdio: "inherit", shell: isWin, ...opts });
  if (r.status !== 0) {
    console.error(`ERROR: ${cmd} ${params.join(" ")} exited ${r.status}`);
    process.exit(r.status ?? 1);
  }
}

function shCap(cmd, params) {
  const r = spawnSync(cmd, params, { cwd: root, encoding: "utf8", shell: isWin });
  return ((r.stdout || "") + (r.stderr || "")).trim();
}

function pkgVersion() {
  return JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
}

function gitShortSha() {
  return shCap("git", ["rev-parse", "--short", "HEAD"]);
}

function gitBranch() {
  const b = shCap("git", ["rev-parse", "--abbrev-ref", "HEAD"]);
  return b === "HEAD" ? "main" : b;
}

const ver = pkgVersion();
const sha = gitShortSha();
const branch = gitBranch();
const distDir = join(root, "dist");
const distDevDir = join(distDir, "dev");
mkdirSync(distDir, { recursive: true });
if (dev) mkdirSync(distDevDir, { recursive: true });

console.log(`release-build: ver=${ver} sha=${sha} branch=${branch} channel=${dev ? "dev" : "stable"}`);

const artifacts = [];

function copyAndRecord(src, destName) {
  const target = dev ? join(distDevDir, destName) : join(distDir, destName);
  copyFileSync(src, target);
  artifacts.push(target);
  console.log(`  + ${target}  (${(statSync(target).size / 1024 / 1024).toFixed(1)} MB)`);
}

if (!skipHost) {
  if (!skipRebuild) {
    console.log("→ tauri build (host)");
    sh("bun", ["run", "tauri", "build"]);
  } else {
    console.log("→ host: --skip-rebuild, reusing existing bundle");
  }

  if (isWin) {
    const nsisDir = join(root, "src-tauri", "target", "release", "bundle", "nsis");
    if (existsSync(nsisDir)) {
      const exe = readdirSync(nsisDir).find((f) => f.endsWith("-setup.exe"));
      if (exe) copyAndRecord(join(nsisDir, exe), dev ? `wtranscriber-setup-${branch}.exe` : `wtranscriber-setup-${ver}.exe`);
    }
  } else {
    const debDir = join(root, "src-tauri", "target", "release", "bundle", "deb");
    if (existsSync(debDir)) {
      const deb = readdirSync(debDir).find((f) => f.endsWith(".deb"));
      if (deb) copyAndRecord(join(debDir, deb), dev ? `wtranscriber-${branch}_amd64.deb` : `wtranscriber_${ver}_amd64.deb`);
    }
    const rpmDir = join(root, "src-tauri", "target", "release", "bundle", "rpm");
    if (existsSync(rpmDir)) {
      const rpm = readdirSync(rpmDir).find((f) => f.endsWith(".rpm"));
      if (rpm) copyAndRecord(join(rpmDir, rpm), dev ? `wtranscriber-${branch}.x86_64.rpm` : `wtranscriber-${ver}.x86_64.rpm`);
    }
    const appimageDir = join(root, "src-tauri", "target", "release", "bundle", "appimage");
    if (existsSync(appimageDir)) {
      const ai = readdirSync(appimageDir).find((f) => f.endsWith(".AppImage"));
      if (ai) copyAndRecord(join(appimageDir, ai), dev ? `wtranscriber-${branch}.AppImage` : `wtranscriber-${ver}.AppImage`);
    }
  }
}

if (!skipAndroid) {
  if (!skipRebuild) {
    sh("bun", ["scripts/patch-android-signing.mjs"]);
    console.log("→ tauri android build (release)");
    sh("bun", ["scripts/android.mjs", "build", "--target=aarch64", "--release"]);
  } else {
    console.log("→ android: --skip-rebuild, reusing existing apk");
  }

  const apkDir = join(root, "src-tauri", "gen", "android", "app", "build", "outputs", "apk", "universal", "release");
  const signedApk = join(apkDir, "app-universal-release.apk");
  const unsignedApk = join(apkDir, "app-universal-release-unsigned.apk");

  let apkSrc = null;
  let signed = false;

  if (existsSync(signedApk)) {
    apkSrc = signedApk;
    signed = true;
  } else if (existsSync(unsignedApk)) {
    const ksProps = join(root, "src-tauri", "gen", "android", "keystore.properties");
    if (existsSync(ksProps)) {
      console.log("→ signing APK with keystore.properties");
      const props = Object.fromEntries(
        readFileSync(ksProps, "utf8")
          .split("\n")
          .filter((l) => l.includes("="))
          .map((l) => l.split("=", 2).map((s) => s.trim())),
      );
      const aligned = join(apkDir, "app-universal-release-aligned.apk");
      const out = join(apkDir, "app-universal-release.apk");
      const sdk = process.env.ANDROID_HOME ?? join(process.env.LOCALAPPDATA ?? "", "Android", "Sdk");
      const buildTools = join(sdk, "build-tools");
      const btVer = readdirSync(buildTools).sort().pop();
      const zipalign = join(buildTools, btVer, isWin ? "zipalign.exe" : "zipalign");
      const apksigner = join(buildTools, btVer, isWin ? "apksigner.bat" : "apksigner");
      sh(zipalign, ["-f", "-p", "4", unsignedApk, aligned]);
      sh(apksigner, [
        "sign",
        "--ks", props.storeFile,
        "--ks-pass", `pass:${props.storePassword}`,
        "--ks-key-alias", props.keyAlias,
        "--key-pass", `pass:${props.keyPassword}`,
        "--out", out,
        aligned,
      ]);
      apkSrc = out;
      signed = true;
    } else {
      apkSrc = unsignedApk;
      signed = false;
    }
  }

  if (apkSrc) {
    if (!signed && !dev) {
      console.error("ERROR: refusing to publish unsigned APK on stable channel.");
      console.error("Create src-tauri/gen/android/keystore.properties with storeFile=, storePassword=, keyAlias=, keyPassword=");
      process.exit(1);
    }
    if (!signed) console.warn("⚠  APK is UNSIGNED — Android will refuse to install. Configure keystore.properties for distributable builds.");
    copyAndRecord(apkSrc, dev ? `wtranscriber-${branch}.apk` : `wtranscriber-${ver}.apk`);
  } else {
    console.warn("⚠  no APK produced");
  }
}

if (artifacts.length === 0) {
  console.error("ERROR: no artifacts produced");
  process.exit(1);
}

const sumsPath = dev ? join(distDevDir, "SHA256SUMS") : join(distDir, `SHA256SUMS-${ver}`);
const lines = artifacts.map((p) => {
  const h = createHash("sha256").update(readFileSync(p)).digest("hex");
  return `${h}  ${basename(p)}`;
});
writeFileSync(sumsPath, lines.join("\n") + "\n");
artifacts.push(sumsPath);
console.log(`  + ${sumsPath}`);

const tauriKey = process.env.TAURI_SIGNING_PRIVATE_KEY;
if (tauriKey) {
  console.log("→ TAURI_SIGNING_PRIVATE_KEY set — generating updater signatures (.sig)");
  for (const p of [...artifacts]) {
    if (p.endsWith(".exe") || p.endsWith(".AppImage") || p.endsWith(".deb") || p.endsWith(".apk")) {
      const r = spawnSync("bun", ["run", "tauri", "signer", "sign", "--private-key", tauriKey, p], {
        cwd: root,
        stdio: "inherit",
        shell: isWin,
        env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD ?? "" },
      });
      if (r.status === 0 && existsSync(`${p}.sig`)) artifacts.push(`${p}.sig`);
    }
  }
}

const manifestPath = dev ? join(distDevDir, "release-manifest.json") : join(distDir, `release-manifest-${ver}.json`);
writeFileSync(
  manifestPath,
  JSON.stringify(
    {
      channel: dev ? "dev" : "stable",
      version: ver,
      branch,
      sha,
      builtAt: new Date().toISOString(),
      artifacts: artifacts.map((p) => basename(p)),
    },
    null,
    2,
  ) + "\n",
);
artifacts.push(manifestPath);
console.log(`  + ${manifestPath}`);

writeFileSync(join(root, "dist", dev ? ".release-dev-artifacts" : ".release-stable-artifacts"), artifacts.join("\n") + "\n");
console.log(`✓ release-build done (${artifacts.length} files)`);
