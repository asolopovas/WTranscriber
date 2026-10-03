import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

describe.skipIf(process.platform !== "linux")("install bootstrap", () => {
  let root: string;
  let bin: string;
  let env: NodeJS.ProcessEnv;

  const mock = (name: string, body: string) =>
    writeFileSync(join(bin, name), `#!/bin/bash\nset -eu\n${body}\n`, { mode: 0o755 });

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), "wt bootstrap "));
    bin = join(root, "tools");
    mkdirSync(bin);
    mkdirSync(join(root, "scripts"));
    copyFileSync(resolve("scripts/install.sh"), join(root, "scripts", "install.sh"));
    for (const name of ["bash", "sh", "dirname", "mkdir", "mktemp", "rm", "grep", "cat", "chmod"]) {
      symlinkSync(`/bin/${name}`, join(bin, name));
    }
    env = {
      PATH: bin,
      HOME: join(root, "home"),
      TEST_LOG: join(root, "commands"),
      TEST_BIN: bin,
    };
    mock("uname", 'if [[ "$1" == -s ]]; then echo Linux; else echo x86_64; fi');
    mock("sudo", 'shift\nexec "$@"');
    mock("apt-get", 'echo "apt-get $*" >> "$TEST_LOG"');
    mock(
      "curl",
      `
echo 'curl' >> "$TEST_LOG"
case "$*" in
    *bun.sh/install*)
        cat > "\${@: -1}" <<'INSTALL'
mkdir -p "$HOME/.bun/bin"
cat > "$HOME/.bun/bin/bun" <<'BUN'
#!/bin/bash
echo "bun $*" >> "$TEST_LOG"
if [[ "$1" == install ]]; then exit "\${TEST_DEPS_STATUS:-0}"; fi
if [[ "$1" == scripts/run.ts ]]; then exit "\${TEST_BUILD_STATUS:-0}"; fi
BUN
chmod +x "$HOME/.bun/bin/bun"
INSTALL
        ;;
    *sh.rustup.rs*)
        cat > "\${@: -1}" <<'INSTALL'
mkdir -p "$HOME/.cargo/bin"
cat > "$HOME/.cargo/bin/rustup" <<'RUSTUP'
#!/bin/bash
echo "rustup $*" >> "$TEST_LOG"
RUSTUP
chmod +x "$HOME/.cargo/bin/rustup"
INSTALL
        ;;
    *) exit 1 ;;
esac`,
    );
  });

  afterEach(() => rmSync(root, { recursive: true, force: true }));

  const run = (...args: string[]) =>
    spawnSync("/bin/bash", [join(root, "scripts", "install.sh"), ...args], {
      env,
      encoding: "utf8",
    });

  it("bootstraps missing Bun and Rust in the same invocation before building and installing", () => {
    const result = run();
    expect(result.status, result.stderr).toBe(0);
    const commands = readFileSync(env.TEST_LOG!, "utf8");
    expect(commands).toContain("apt-get update");
    expect(commands).toContain("bun install\n");
    expect(commands).toContain("rustup show active-toolchain");
    expect(commands).toContain("bun scripts/desktop.ts build");
    expect(commands.trim().endsWith("bun scripts/install-linux.ts")).toBe(true);
  });

  it.each(["TEST_DEPS_STATUS", "TEST_BUILD_STATUS"])(
    "stops on %s failure without installing stale output",
    (variable) => {
      env[variable] = "7";
      expect(run().status).toBe(7);
      expect(readFileSync(env.TEST_LOG!, "utf8")).not.toContain("bun scripts/install-linux.ts");
    },
  );

  it("handles help, invalid options and unsupported hosts before bootstrap", () => {
    expect(run("--help").status).toBe(0);
    expect(run("--interactive").status).toBe(2);
    expect(existsSync(env.TEST_LOG!)).toBe(false);
    mock("uname", "echo Darwin");
    expect(run().stderr).toContain("currently supports Windows and Linux");
    expect(existsSync(env.TEST_LOG!)).toBe(false);
  });

  it("blocks a recorded dev session before changing system dependencies", () => {
    mkdirSync(join(root, "tmp"));
    writeFileSync(join(root, "tmp", "_pids.json"), "{}");
    expect(run().stderr).toContain("just stop");
    expect(existsSync(env.TEST_LOG!)).toBe(false);
  });
});
