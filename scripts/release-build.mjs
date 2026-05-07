#!/usr/bin/env bun
import { existsSync, mkdirSync, copyFileSync, readFileSync, writeFileSync, statSync, readdirSync } from "node:fs";
import { join, basename } from "node:path";
import { spawnSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { platform } from "node:os";

const root = join(import.meta.dir, "..");
const isWin = platform() === "win32";

const argv = new Set(process.argv.slice(2));
const dev = argv.has("--dev");
const skipAndroid = argv.has("--no-android");
const skipHost = argv.has("--no-host");
const skipWsl = argv.has("--no-wsl");
const skipRebuild = argv.has("--skip-rebuild");
const sequential = argv.has("--sequential");

const fastEnv = {
  CARGO_INCREMENTAL: "1",
  CARGO_NET_GIT_FETCH_WITH_CLI: "true",
};

function shCap(cmd, params, opts = {}) {
  const r = spawnSync(cmd, params, { cwd: root, encoding: "utf8", shell: isWin, ...opts });
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

function runStreamed(tag, cmd, args, opts = {}) {
  return new Promise((resolve) => {
    const child = spawn(cmd, args, {
      cwd: root,
      shell: isWin,
      env: { ...process.env, ...fastEnv, ...(opts.env || {}) },
      stdio: ["ignore", "pipe", "pipe"],
      ...opts,
    });
    const prefix = `[${tag}] `;
    const pipeLines = (stream) => {
      let buf = "";
      stream.setEncoding("utf8");
      stream.on("data", (chunk) => {
        buf += chunk;
        const lines = buf.split("\n");
        buf = lines.pop() ?? "";
        for (const line of lines) process.stdout.write(prefix + line + "\n");
      });
      stream.on("end", () => {
        if (buf.length) process.stdout.write(prefix + buf + "\n");
      });
    };
    pipeLines(child.stdout);
    pipeLines(child.stderr);
    child.on("error", (e) => {
      process.stdout.write(prefix + `spawn error: ${e.message}\n`);
      resolve(1);
    });
    child.on("exit", (code) => resolve(code ?? 0));
  });
}

const ver = pkgVersion();
const sha = gitShortSha();
const branch = gitBranch();
const outDir = join(root, "releases");
const outDevDir = join(outDir, "dev");

console.log(`release-build: ver=${ver} sha=${sha} branch=${branch} channel=${dev ? "dev" : "stable"} parallel=${!sequential}`);

const artifacts = [];

function copyAndRecord(src, destName) {
  const destDir = dev ? outDevDir : outDir;
  mkdirSync(destDir, { recursive: true });
  const target = join(destDir, destName);
  copyFileSync(src, target);
  artifacts.push(target);
  console.log(`  + ${target}  (${(statSync(target).size / 1024 / 1024).toFixed(1)} MB)`);
}

async function preWarm() {
  if (skipRebuild) return;
  console.log("→ pre-warm: vite build (one-time, shared across all platforms)");
  const rc = await runStreamed("vite", "bun", ["run", "build"]);
  if (rc !== 0) throw new Error(`vite build failed (exit ${rc})`);

  console.log("→ pre-warm: cargo fetch");
  await runStreamed("fetch", "cargo", ["fetch", "--manifest-path", "src-tauri/Cargo.toml"]);
}

const skipBeforeBuild = ["--config", '{"build":{"beforeBuildCommand":""}}'];

async function buildHost() {
  if (skipRebuild) {
    console.log("[host] --skip-rebuild, reusing existing bundle");
    return 0;
  }
  return runStreamed("host", "bun", ["run", "tauri", "build", ...skipBeforeBuild]);
}

async function buildAndroid() {
  if (skipRebuild) {
    console.log("[and] --skip-rebuild, reusing existing apk");
    return 0;
  }
  await runStreamed("and", "bun", ["scripts/patch-android-signing.mjs"]);
  return runStreamed("and", "bun", ["run", "tauri", "android", "build", "--target", "aarch64", "--apk", ...skipBeforeBuild]);
}

async function buildWsl() {
  if (skipRebuild) {
    console.log("[wsl] --skip-rebuild, looking for existing .deb");
    return 0;
  }
  const probe = spawnSync(
    "wsl",
    ["--", "bash", "-lc", "command -v bun >/dev/null && command -v cargo >/dev/null && echo READY"],
    { encoding: "utf8" },
  );
  if ((probe.stdout || "").trim() !== "READY") {
    console.log("[wsl] skipping (no distro with bun + cargo — run 'just wsl-doctor')");
    return -1;
  }
  const wslRoot = spawnSync("wsl", ["--", "wslpath", "-u", root], { encoding: "utf8" }).stdout.trim();
  if (!wslRoot) return -1;
  const buildCmd = [
    `cd "${wslRoot}"`,
    'export CARGO_TARGET_DIR="$HOME/.cache/wtranscriber-wsl-target"',
    "export CARGO_INCREMENTAL=1",
    'mkdir -p "$CARGO_TARGET_DIR"',
    "bun install --frozen-lockfile --no-progress 2>&1 | tail -5",
    `bun run tauri build --bundles deb ${skipBeforeBuild.map((a) => (a.includes(" ") || a.includes('"') ? `'${a}'` : a)).join(" ")}`,
  ].join(" && ");
  return runStreamed("wsl", "wsl", ["--", "bash", "-lc", buildCmd]);
}

function findHostBundle() {
  if (isWin) {
    const nsisDir = join(root, "src-tauri", "target", "release", "bundle", "nsis");
    if (existsSync(nsisDir)) {
      const exe = readdirSync(nsisDir).find((f) => f.endsWith("-setup.exe"));
      if (exe) return { src: join(nsisDir, exe), name: dev ? `wtranscriber-setup-${branch}.exe` : `wtranscriber-setup-${ver}.exe` };
    }
    return null;
  }
  const debDir = join(root, "src-tauri", "target", "release", "bundle", "deb");
  if (existsSync(debDir)) {
    const deb = readdirSync(debDir).find((f) => f.endsWith(".deb"));
    if (deb) return { src: join(debDir, deb), name: dev ? `wtranscriber-${branch}_amd64.deb` : `wtranscriber_${ver}_amd64.deb` };
  }
  return null;
}

function findWslDeb() {
  const find = spawnSync(
    "wsl",
    ["--", "bash", "-lc", 'ls "$HOME/.cache/wtranscriber-wsl-target/release/bundle/deb/"*.deb 2>/dev/null | head -1'],
    { encoding: "utf8" },
  );
  const wslPath = (find.stdout || "").trim();
  if (!wslPath) return null;
  const wp = spawnSync("wsl", ["--", "wslpath", "-w", wslPath], { encoding: "utf8" });
  const winPath = (wp.stdout || "").trim();
  if (!winPath) return null;
  return { src: winPath, name: dev ? `wtranscriber-${branch}_amd64.deb` : `wtranscriber_${ver}_amd64.deb` };
}

function findApk() {
  const apkDir = join(root, "src-tauri", "gen", "android", "app", "build", "outputs", "apk", "universal", "release");
  const signed = join(apkDir, "app-universal-release.apk");
  const unsigned = join(apkDir, "app-universal-release-unsigned.apk");
  if (existsSync(signed)) return { src: signed, signed: true };
  if (existsSync(unsigned)) {
    const ksProps = join(root, "src-tauri", "gen", "android", "keystore.properties");
    if (existsSync(ksProps)) {
      const props = Object.fromEntries(
        readFileSync(ksProps, "utf8")
          .split(/\r?\n/)
          .filter((l) => l.includes("="))
          .map((l) => l.split("=", 2).map((s) => s.trim())),
      );
      const sdk = process.env.ANDROID_HOME ?? join(process.env.LOCALAPPDATA ?? "", "Android", "Sdk");
      const buildTools = join(sdk, "build-tools");
      const btVer = readdirSync(buildTools).sort().pop();
      const zipalign = join(buildTools, btVer, isWin ? "zipalign.exe" : "zipalign");
      const apksigner = join(buildTools, btVer, isWin ? "apksigner.bat" : "apksigner");
      const aligned = join(apkDir, "app-universal-release-aligned.apk");
      const out = join(apkDir, "app-universal-release.apk");
      const ra = spawnSync(zipalign, ["-f", "-p", "4", unsigned, aligned], { stdio: "inherit", shell: isWin });
      if (ra.status !== 0) return { src: unsigned, signed: false };
      const rs = spawnSync(
        apksigner,
        [
          "sign",
          "--ks", props.storeFile,
          "--ks-pass", `pass:${props.storePassword}`,
          "--ks-key-alias", props.keyAlias,
          "--key-pass", `pass:${props.keyPassword}`,
          "--out", out,
          aligned,
        ],
        { stdio: "inherit", shell: isWin },
      );
      if (rs.status !== 0) return { src: unsigned, signed: false };
      return { src: out, signed: true };
    }
    return { src: unsigned, signed: false };
  }
  return null;
}

(async () => {
  await preWarm();

  const tasks = [];
  if (!skipHost) tasks.push({ name: "host", fn: buildHost });
  if (!skipAndroid) tasks.push({ name: "and", fn: buildAndroid });
  if (!skipWsl && isWin) tasks.push({ name: "wsl", fn: buildWsl });

  console.log(`→ launching ${tasks.length} builds ${sequential ? "sequentially" : "in parallel"}`);
  const results = {};
  if (sequential) {
    for (const t of tasks) results[t.name] = await t.fn();
  } else {
    const all = await Promise.all(tasks.map((t) => t.fn().then((rc) => [t.name, rc])));
    for (const [n, rc] of all) results[n] = rc;
  }

  const fail = (msg) => {
    console.error(`ERROR: ${msg}`);
    process.exit(1);
  };

  if (!skipHost) {
    if (results.host !== 0) fail(`host build failed (exit ${results.host})`);
    const found = findHostBundle();
    if (found) copyAndRecord(found.src, found.name);
  }

  if (!skipWsl && isWin && results.wsl !== undefined) {
    if (results.wsl === -1) {
      console.warn("⚠  WSL build skipped (no distro with bun + cargo)");
    } else if (results.wsl !== 0) {
      console.warn(`⚠  WSL build failed (exit ${results.wsl}); continuing without .deb`);
    } else {
      const found = findWslDeb();
      if (found) copyAndRecord(found.src, found.name);
    }
  }

  if (!skipAndroid) {
    if (results.and !== 0) fail(`android build failed (exit ${results.and})`);
    const apk = findApk();
    if (apk) {
      if (!apk.signed && !dev) fail("refusing to publish unsigned APK on stable channel. Configure src-tauri/gen/android/keystore.properties.");
      if (!apk.signed) console.warn("⚠  APK is UNSIGNED — Android will refuse to install. Configure keystore.properties for distributable builds.");
      copyAndRecord(apk.src, dev ? `wtranscriber-${branch}.apk` : `wtranscriber-${ver}.apk`);
    } else {
      console.warn("⚠  no APK produced");
    }
  }

  if (artifacts.length === 0) fail("no artifacts produced");

  const channelDir = dev ? outDevDir : outDir;
  mkdirSync(channelDir, { recursive: true });

  const sumsPath = dev ? join(outDevDir, "SHA256SUMS") : join(outDir, `SHA256SUMS-${ver}`);
  const lines = artifacts.map((p) => `${createHash("sha256").update(readFileSync(p)).digest("hex")}  ${basename(p)}`);
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

  const manifestPath = dev ? join(outDevDir, "release-manifest.json") : join(outDir, `release-manifest-${ver}.json`);
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

  writeFileSync(join(outDir, dev ? ".release-dev-artifacts" : ".release-stable-artifacts"), artifacts.join("\n") + "\n");
  console.log(`✓ release-build done (${artifacts.length} files)`);
})().catch((e) => {
  console.error(`FATAL: ${e.message}`);
  process.exit(1);
});
