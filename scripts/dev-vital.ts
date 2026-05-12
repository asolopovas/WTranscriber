#!/usr/bin/env bun
import {
  appendFileSync,
  openSync,
  closeSync,
  statSync,
  openSync as _o,
  readSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = dirname(dirname(fileURLToPath(import.meta.url)));
const tmp = join(repoRoot, "tmp");
const outPath = join(tmp, "dev-vital.log");
const pkg = "com.asolopovas.wtranscriber";

const sources: { src: string; path: string; offset: number; carry: string }[] = [
  { src: "dev", path: join(tmp, "android-dev.log"), offset: 0, carry: "" },
  { src: "devErr", path: join(tmp, "android-dev.err.log"), offset: 0, carry: "" },
  { src: "logcat", path: join(tmp, "logcat.log"), offset: 0, carry: "" },
];

writeFileSync(outPath, "");

const viteRe = /\[vite\] (hmr update|error|page reload)|panic| ERROR | WARN /;
const rustRe = /RustStdoutStderr/;
const rustContentRe = /panic|error|warn/i;
const chromiumRe = /chromium/;
const chromiumContentRe = /console\.cc.*(ERROR|WARNING)/;
const ipcRe = /tauri::ipc|permission denied|not allowed by ACL/;
const amRe = /am_(crash|proc_died|kill)/;

function filterDev(line: string): boolean {
  return viteRe.test(line);
}

function filterLogcat(line: string): boolean {
  if (rustRe.test(line) && rustContentRe.test(line)) return true;
  if (amRe.test(line) && line.includes(pkg)) return true;
  if (chromiumRe.test(line) && chromiumContentRe.test(line)) return true;
  if (ipcRe.test(line)) return true;
  return false;
}

function readNew(s: { src: string; path: string; offset: number; carry: string }): string[] {
  let st;
  try {
    st = statSync(s.path);
  } catch {
    return [];
  }
  if (st.size < s.offset) {
    s.offset = 0;
    s.carry = "";
  }
  if (st.size === s.offset) return [];
  const fd = openSync(s.path, "r");
  const len = st.size - s.offset;
  const buf = Buffer.alloc(len);
  readSync(fd, buf, 0, len, s.offset);
  closeSync(fd);
  s.offset = st.size;
  const text = s.carry + buf.toString("utf8");
  const lines = text.split(/\r?\n/);
  s.carry = lines.pop() ?? "";
  return lines;
}

let stopping = false;
function shutdown() {
  stopping = true;
  process.exit(0);
}
process.on("SIGTERM", shutdown);
process.on("SIGINT", shutdown);

async function loop() {
  while (!stopping) {
    for (const s of sources) {
      const lines = readNew(s);
      for (const line of lines) {
        if (!line) continue;
        const keep = s.src === "logcat" ? filterLogcat(line) : filterDev(line);
        if (keep) {
          try {
            appendFileSync(outPath, `[${s.src}] ${line}\n`);
          } catch {}
        }
      }
    }
    await Bun.sleep(250);
  }
}

loop();
