#!/usr/bin/env bun
import { Glob } from "bun";
import { existsSync, statSync } from "node:fs";
import { dirname, extname, isAbsolute, join, normalize, relative, resolve, sep } from "node:path";

const ROOT = process.cwd();
const MAX_AGENTS_LINES = 100;
const PLAN_DIRS = ["active", "completed", "abandoned"] as const;
const PLAN_HEADINGS = [
  "## Goal",
  "## Acceptance criteria",
  "## Current context",
  "## Steps",
  "## Decisions",
  "## Verification log",
  "## Handoff notes",
];

let failures = 0;

const fail = (message: string): void => {
  console.error(message);
  failures++;
};

const rel = (path: string): string => relative(ROOT, path).split(sep).join("/");

const insideRoot = (path: string): boolean => {
  const fromRoot = relative(ROOT, path);
  return fromRoot === "" || (!fromRoot.startsWith("..") && !isAbsolute(fromRoot));
};

const textFile = async (path: string): Promise<string> => Bun.file(path).text();

const markdownFiles = async (): Promise<string[]> => {
  const files = [resolve(ROOT, "AGENTS.md")];
  const glob = new Glob("docs/**/*.md");
  for await (const file of glob.scan({ cwd: ROOT, absolute: true })) files.push(file);
  return files.sort();
};

const slug = (heading: string): string =>
  heading
    .replace(/^#+\s*/, "")
    .trim()
    .toLowerCase()
    .replace(/`([^`]+)`/g, "$1")
    .replace(/[^a-z0-9\s-]/g, "")
    .replace(/\s+/g, "-");

const headingSlugs = (text: string): Set<string> => {
  const seen = new Map<string, number>();
  const out = new Set<string>();
  for (const line of text.split(/\r?\n/)) {
    if (!/^#{1,6}\s+/.test(line)) continue;
    const base = slug(line);
    const count = seen.get(base) ?? 0;
    seen.set(base, count + 1);
    out.add(count === 0 ? base : `${base}-${count}`);
  }
  return out;
};

const isExternal = (target: string): boolean =>
  /^[a-z][a-z0-9+.-]*:/i.test(target) || target.startsWith("//");

const stripQuery = (target: string): string => target.split(/[?#]/, 1)[0] ?? "";

const anchor = (target: string): string => {
  const index = target.indexOf("#");
  return index === -1 ? "" : decodeURIComponent(target.slice(index + 1));
};

const validateLinks = async (files: string[]): Promise<void> => {
  const textByFile = new Map<string, string>();
  for (const file of files) textByFile.set(file, await textFile(file));

  for (const file of files) {
    const text = textByFile.get(file) ?? "";
    const linkRe = /\[[^\]\n]+\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g;
    let match: RegExpExecArray | null;
    while ((match = linkRe.exec(text)) !== null) {
      const target = match[1] ?? "";
      if (!target || isExternal(target)) continue;
      const line = text.slice(0, match.index).split(/\r?\n/).length;
      const local = stripQuery(target);
      const targetPath = local === "" ? file : resolve(dirname(file), decodeURIComponent(local));
      if (!insideRoot(targetPath)) {
        fail(`${rel(file)}:${line} [docs-link] local link escapes repository: ${target}`);
        continue;
      }
      if (local !== "" && !existsSync(targetPath)) {
        fail(`${rel(file)}:${line} [docs-link] broken local link: ${target}`);
        continue;
      }
      const linkAnchor = anchor(target);
      if (linkAnchor && extname(targetPath) === ".md") {
        const targetText =
          textByFile.get(targetPath) ?? (existsSync(targetPath) ? await textFile(targetPath) : "");
        if (!headingSlugs(targetText).has(linkAnchor)) {
          fail(
            `${rel(file)}:${line} [docs-anchor] missing heading #${linkAnchor} in ${rel(targetPath)}`,
          );
        }
      }
    }
  }
};

const validateAgentsMap = async (): Promise<void> => {
  const path = resolve(ROOT, "AGENTS.md");
  const text = await textFile(path);
  const lines = text
    .split(/\r?\n/)
    .filter((line, index, arr) => index < arr.length - 1 || line !== "");
  if (lines.length > MAX_AGENTS_LINES) {
    fail(
      `AGENTS.md:1 [agents-size] keep AGENTS.md as a table of contents (${lines.length}/${MAX_AGENTS_LINES} lines); move detail into docs/`,
    );
  }
  for (const required of [
    "docs/README.md",
    "docs/architecture.md",
    "docs/dev-loop.md",
    "docs/verification.md",
    "docs/quality.md",
    "docs/technical-debt.md",
    "docs/plans/README.md",
  ]) {
    if (!text.includes(required)) fail(`AGENTS.md:1 [agents-map] add a pointer to ${required}`);
  }
};

const validateDocsCatalogue = async (files: string[]): Promise<void> => {
  const catalogue = await textFile(resolve(ROOT, "docs/README.md"));
  const topLevelDocs = files
    .map((file) => rel(file))
    .filter(
      (file) =>
        file.startsWith("docs/") && file !== "docs/README.md" && !file.startsWith("docs/plans/"),
    );
  for (const file of topLevelDocs) {
    const link = file.slice("docs/".length);
    if (!catalogue.includes(`](${link})`)) {
      fail(`docs/README.md:1 [docs-catalogue] catalogue ${file} so agents can discover it`);
    }
  }
};

const validatePlans = async (): Promise<void> => {
  const root = resolve(ROOT, "docs/plans");
  for (const dir of PLAN_DIRS) {
    const path = join(root, dir);
    if (!existsSync(path) || !statSync(path).isDirectory()) {
      fail(`docs/plans/README.md:1 [plans-dir] create docs/plans/${dir}/`);
    }
  }
  const glob = new Glob("docs/plans/{active,completed,abandoned}/*.md");
  for await (const file of glob.scan({ cwd: ROOT, absolute: true })) {
    const text = await textFile(file);
    const planDir = normalize(dirname(file)).split(sep).at(-1) ?? "";
    const status = text.match(/^Status:\s*(\w+)/m)?.[1]?.toLowerCase();
    if (status !== planDir) {
      fail(
        `${rel(file)}:1 [plan-status] set \`Status: ${planDir}\` or move this plan to docs/plans/${status ?? "<status>"}/`,
      );
    }
    for (const heading of PLAN_HEADINGS) {
      if (!text.includes(heading)) fail(`${rel(file)}:1 [plan-template] missing ${heading}`);
    }
  }
};

const main = async (): Promise<void> => {
  const files = await markdownFiles();
  await validateAgentsMap();
  await validateDocsCatalogue(files);
  await validatePlans();
  await validateLinks(files);
  if (failures > 0) {
    console.error(`\n✗ lint-docs: ${failures} issue(s)`);
    process.exit(1);
  }
  console.log("✓ lint-docs");
};

await main();
