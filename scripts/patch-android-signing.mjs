#!/usr/bin/env bun
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";

const root = join(import.meta.dir, "..");
const gradle = join(root, "src-tauri", "gen", "android", "app", "build.gradle.kts");

if (!existsSync(gradle)) {
  console.log("patch-android-signing: gen/android not found — run 'just android-init' first");
  process.exit(0);
}

const MARKER = "// wtranscriber: keystore-signing-patch v1";
const raw = readFileSync(gradle, "utf8");

if (raw.includes(MARKER)) {
  console.log("patch-android-signing: already patched");
  process.exit(0);
}

const eolMatch = raw.match(/\r\n|\n/);
const EOL = eolMatch ? eolMatch[0] : "\n";
const lines = raw.split(/\r?\n/);

function findLine(predicate, from = 0) {
  for (let i = from; i < lines.length; i++) if (predicate(lines[i])) return i;
  return -1;
}

const tauriEnd = findLine((l) => l.trim() === "}", findLine((l) => l.includes("val tauriProperties")));
if (tauriEnd === -1) {
  console.error("patch-android-signing: could not locate tauriProperties block");
  process.exit(1);
}

const propsBlock = [
  "",
  MARKER,
  "val keystoreProperties = Properties().apply {",
  '    val propFile = rootProject.file("keystore.properties")',
  "    if (propFile.exists()) {",
  "        propFile.inputStream().use { load(it) }",
  "    }",
  "}",
];
lines.splice(tauriEnd + 1, 0, ...propsBlock);

const buildTypesIdx = findLine((l) => l.trim() === "buildTypes {");
if (buildTypesIdx === -1) {
  console.error("patch-android-signing: could not locate buildTypes block");
  process.exit(1);
}
const signingBlock = [
  "    signingConfigs {",
  '        if (keystoreProperties.containsKey("storeFile")) {',
  '            create("release") {',
  '                storeFile = file(keystoreProperties.getProperty("storeFile"))',
  '                storePassword = keystoreProperties.getProperty("storePassword")',
  '                keyAlias = keystoreProperties.getProperty("keyAlias")',
  '                keyPassword = keystoreProperties.getProperty("keyPassword")',
  "                enableV2Signing = true",
  "                enableV3Signing = true",
  "            }",
  "        }",
  "    }",
];
lines.splice(buildTypesIdx, 0, ...signingBlock);

const releaseIdx = findLine((l) => l.includes('getByName("release")'), buildTypesIdx);
if (releaseIdx === -1) {
  console.error("patch-android-signing: could not locate release buildType");
  process.exit(1);
}
lines.splice(releaseIdx + 1, 0,
  '            if (keystoreProperties.containsKey("storeFile")) {',
  '                signingConfig = signingConfigs.getByName("release")',
  "            }",
);

writeFileSync(gradle, lines.join(EOL));
console.log("patch-android-signing: applied keystore-signing patch");
