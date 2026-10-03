import { spawnSync } from "node:child_process";
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  readlinkSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { installLinux } from "./install-linux";

describe.skipIf(process.platform !== "linux")("Linux local install", () => {
  let root: string;
  let prefix: string;
  let data: string;
  let release: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), "wt-install-"));
    prefix = join(root, "user's local $files");
    data = join(root, "desktop data");
    release = join(root, "src-tauri", "target", "release");
    mkdirSync(release, { recursive: true });
    mkdirSync(join(root, "src-tauri", "icons"));
    writeFileSync(join(root, "src-tauri", "icons", "128x128.png"), "icon");
    for (const name of ["wtranscriber", "wt"]) {
      writeFileSync(join(release, name), '#!/bin/sh\nprintf "%s\\n" "$@"\n', {
        mode: 0o755,
      });
    }
    writeFileSync(join(release, "libonnxruntime.so.1"), "runtime");
  });

  afterEach(() => rmSync(root, { recursive: true, force: true }));

  it("installs runnable launchers with quoted paths and arguments, runtimes and a desktop entry", () => {
    installLinux(root, prefix, data);
    const cli = spawnSync(join(prefix, "bin", "wt"), ["recording with spaces.wav", "$value"], {
      encoding: "utf8",
    });
    expect(cli.status).toBe(0);
    expect(cli.stdout).toBe("recording with spaces.wav\n$value\n");
    const current = join(prefix, "lib", "wtranscriber", "current");
    expect(readFileSync(join(current, "libonnxruntime.so.1"), "utf8")).toBe("runtime");
    const desktop = readFileSync(
      join(data, "applications", "com.asolopovas.wtranscriber.desktop"),
      "utf8",
    );
    expect(desktop).toContain('local \\\\$files/bin/wtranscriber"');
    expect(desktop).toContain("Terminal=false");
  });

  it("replaces a previous build only after verifying the new CLI", () => {
    installLinux(root, prefix, data);
    const current = join(prefix, "lib", "wtranscriber", "current");
    const previous = readlinkSync(current);
    writeFileSync(join(release, "wt"), "#!/bin/sh\nexit 9\n");
    expect(() => installLinux(root, prefix, data)).toThrow("Built CLI cannot run");
    expect(readlinkSync(current)).toBe(previous);
    expect(spawnSync(join(prefix, "bin", "wt"), ["--help"]).status).toBe(0);
    expect(existsSync(join(prefix, "lib", "wtranscriber", ".install-lock"))).toBe(false);
    writeFileSync(join(release, "wt"), "#!/bin/sh\nexit 0\n");
    installLinux(root, prefix, data);
    expect(readlinkSync(current)).not.toBe(previous);
    expect(existsSync(previous)).toBe(false);
    expect(
      readdirSync(join(prefix, "lib", "wtranscriber")).filter((n) => n.startsWith("build-")),
    ).toHaveLength(1);
  });

  it("rejects empty binaries and concurrent installs", () => {
    writeFileSync(join(release, "wt"), "");
    expect(() => installLinux(root, prefix, data)).toThrow("Empty build output");
    expect(existsSync(prefix)).toBe(false);
    writeFileSync(join(release, "wt"), "#!/bin/sh\nexit 0\n");
    mkdirSync(join(prefix, "lib", "wtranscriber", ".install-lock"), { recursive: true });
    expect(() => installLinux(root, prefix, data)).toThrow("Install lock exists");
  });
});
