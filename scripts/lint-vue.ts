#!/usr/bin/env bun
import { Glob } from "bun";
import { relative } from "node:path";

interface Rule {
  id: string;
  re: RegExp;
  msg: string;
}

const RULES: Rule[] = [
  {
    id: "style-attr-html-entity-quote",
    re: /\sstyle="[^"]*&quot;[^"]*"/g,
    msg: 'use single quotes inside style="..." (CSS accepts \' for strings); &quot; confuses CSS validators',
  },
  {
    id: "style-attr-html-entity-amp",
    re: /\sstyle="[^"]*&amp;[^"]*"/g,
    msg: 'do not HTML-encode & inside style="..."',
  },
];

const ROOT = process.cwd();
const glob = new Glob("src/**/*.vue");

let failed = 0;
for await (const file of glob.scan({ cwd: ROOT, absolute: true })) {
  const text = await Bun.file(file).text();
  const lines = text.split("\n");
  for (const rule of RULES) {
    rule.re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = rule.re.exec(text)) !== null) {
      const lineNum = text.slice(0, m.index).split("\n").length;
      const col = m.index - text.lastIndexOf("\n", m.index - 1);
      console.error(`${relative(ROOT, file)}:${lineNum}:${col} [${rule.id}] ${rule.msg}`);
      console.error(`  ${(lines[lineNum - 1] ?? "").trim()}`);
      failed++;
    }
  }
}

if (failed > 0) {
  console.error(`\n✗ lint-vue: ${failed} issue(s)`);
  process.exit(1);
}
console.log("✓ lint-vue");
