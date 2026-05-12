#!/usr/bin/env bun
import { spawnSync } from "node:child_process";
import { platform } from "node:os";
import path from "node:path";

type Status = "ok" | "warn" | "fail";
interface CheckResult {
  status: Status;
  detail?: string;
}
type Check = { name: string; fn: () => Promise<CheckResult> | CheckResult };

const checks: Check[] = [];
let failed = 0;

const add = (name: string, fn: () => Promise<CheckResult> | CheckResult): void => {
  checks.push({ name, fn });
};

const which = (cmd: string): string | null => {
  const probe = platform() === "win32" ? "where" : "which";
  const r = spawnSync(probe, [cmd], { encoding: "utf8" });
  if (r.status !== 0) return null;
  return r.stdout.trim().split(/\r?\n/)[0] ?? null;
};

const run = (cmd: string, args: string[]): { ok: boolean; out: string; err: string } => {
  const r = spawnSync(cmd, args, { encoding: "utf8" });
  return {
    ok: r.status === 0,
    out: (r.stdout ?? "").trim(),
    err: (r.stderr ?? "").trim(),
  };
};

const ok = (detail?: string): CheckResult => ({ status: "ok", detail });
const warn = (detail?: string): CheckResult => ({ status: "warn", detail });
const fail = (detail?: string): CheckResult => ({ status: "fail", detail });

add("rustc", () => {
  const r = run("rustc", ["--version"]);
  if (!r.ok) return fail("not found");
  const m = r.out.match(/rustc (\d+)\.(\d+)\.(\d+)/);
  if (!m) return warn(r.out);
  const maj = Number(m[1]);
  const min = Number(m[2]);
  if (maj > 1 || (maj === 1 && min >= 88)) return ok(r.out);
  return fail(`${r.out} (need ≥1.88)`);
});

add("cargo", () => {
  const r = run("cargo", ["--version"]);
  return r.ok ? ok(r.out) : fail("not found");
});

add("bun", () => {
  const p = which("bun");
  if (!p) return fail("not found");
  return ok(`v${Bun.version}  (${p})`);
});

add("just", () => {
  const r = run("just", ["--version"]);
  return r.ok ? ok(r.out) : fail("not found");
});

add("cargo-machete", () =>
  which("cargo-machete") ? ok("present") : warn("missing — `just check` will install it"),
);

add("cargo-audit", () =>
  which("cargo-audit") ? ok("present") : warn("missing — `just check` will install it"),
);

add("MSRV", async () => {
  const f = Bun.file(path.join("src-tauri", "Cargo.toml"));
  if (!(await f.exists())) return fail("Cargo.toml missing");
  const txt = await f.text();
  const m = txt.match(/rust-version\s*=\s*"([^"]+)"/);
  return m ? ok(m[1]!) : warn("no rust-version pin");
});

add("audit.toml", async () =>
  (await Bun.file(".cargo/audit.toml").exists())
    ? ok(".cargo/audit.toml")
    : warn("no project ignore list"),
);

add("git hooks", () => {
  const r = run("git", ["config", "core.hooksPath"]);
  if (r.out === ".githooks") return ok(".githooks");
  return fail(`core.hooksPath=${r.out || "unset"} (run \`just install-hooks\`)`);
});

add("node_modules", async () =>
  (await Bun.file("node_modules/vue/package.json").exists())
    ? ok("present")
    : warn("run `just setup`"),
);

if (platform() === "win32") {
  add("LIBCLANG_PATH", async () => {
    const p = process.env.LIBCLANG_PATH;
    if (!p) return warn("not set (needed for some Rust deps)");
    return (await Bun.file(p).exists()) ? ok(p) : warn(`${p} (missing)`);
  });
  add("cudart64_12.dll", async () => {
    const dll = "C:\\Windows\\System32\\cudart64_12.dll";
    return (await Bun.file(dll).exists())
      ? ok(dll)
      : warn("absent — GPU sidecar will fall back to CPU");
  });
  add("CUDA Toolkit", async () => {
    const cudaPath = process.env.CUDA_PATH;
    if (!cudaPath) {
      return fail(
        "CUDA_PATH not set — default `cuda` feature needs it (install CUDA Toolkit 12.x)",
      );
    }
    const nvcc = path.join(cudaPath, "bin", "nvcc.exe");
    if (!(await Bun.file(nvcc).exists())) {
      return fail(
        `${nvcc} missing — CUDA_PATH points at a non-existent install (reinstall CUDA Toolkit or use --no-default-features --features sherpa-static)`,
      );
    }
    return ok(`${cudaPath}`);
  });
  add("cuDNN", async () => {
    const candidates = [
      "C:\\Windows\\System32\\cudnn64_9.dll",
      path.join(process.env.LOCALAPPDATA ?? "", "Programs", "cuDNN", "v9", "bin", "cudnn64_9.dll"),
    ];
    for (const c of candidates) {
      if (c && (await Bun.file(c).exists())) return ok(c);
    }
    return warn("cudnn64_9.dll not found — `just cudnn` to install");
  });
}

const symbol: Record<Status, string> = { ok: "OK  ", warn: "WARN", fail: "FAIL" };

console.log("WTranscriber doctor — desktop dev prerequisites\n");
for (const c of checks) {
  let r: CheckResult;
  try {
    r = await c.fn();
  } catch (e) {
    r = fail((e as Error).message);
  }
  if (r.status === "fail") failed++;
  console.log(`  ${symbol[r.status]}  ${c.name.padEnd(20)} ${r.detail ?? ""}`);
}
console.log("");
if (failed > 0) {
  console.error(`✗ ${failed} hard failure(s) — fix above before \`just dev\` / \`just check\`.`);
  process.exit(1);
}
console.log("✓ doctor passed");
