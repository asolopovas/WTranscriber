#!/usr/bin/env bun
import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = fileURLToPath(new URL("..", import.meta.url));
const SRC = join(ROOT, "src");

async function* walk(dir) {
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(full);
    else if (entry.isFile() && entry.name.endsWith(".vue")) yield full;
  }
}

const RULES = [
  {
    id: "style-attr-html-entity-quote",
    re: /\sstyle="[^"]*&quot;[^"]*"/g,
    msg: "use single quotes inside style=\"...\" (CSS accepts ' for strings); &quot; confuses CSS validators",
  },
  {
    id: "style-attr-html-entity-amp",
    re: /\sstyle="[^"]*&amp;[^"]*"/g,
    msg: "do not HTML-encode & inside style=\"...\"",
  },
];

let failed = 0;
for await (const file of walk(SRC)) {
  const text = await readFile(file, "utf8");
  const lines = text.split("\n");
  for (const rule of RULES) {
    rule.re.lastIndex = 0;
    let m;
    while ((m = rule.re.exec(text)) !== null) {
      const lineNum = text.slice(0, m.index).split("\n").length;
      const col = m.index - text.lastIndexOf("\n", m.index - 1);
      console.error(
        `${relative(ROOT, file)}:${lineNum}:${col} [${rule.id}] ${rule.msg}`,
      );
      console.error(`  ${lines[lineNum - 1].trim()}`);
      failed++;
    }
  }
}

if (failed > 0) {
  console.error(`\n✗ lint-vue: ${failed} issue(s)`);
  process.exit(1);
}
console.log("✓ lint-vue");
