import { spawnSync } from "node:child_process";
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  lstatSync,
  readFileSync,
  realpathSync,
  mkdtempSync,
  readdirSync,
  readlinkSync,
  renameSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";

const shellQuote = (value: string) => `'${value.replaceAll("'", "'\\''")}'`;
const desktopValue = (value: string) => value.replaceAll("\\", "\\\\");
const desktopExec = (value: string) =>
  desktopValue(`"${value.replace(/[\\"`$]/g, "\\$&").replaceAll("%", "%%")}"`);

function copyLibrary(directory: string, name: string, stage: string): void {
  const source = join(directory, name);
  const destination = join(stage, name);
  rmSync(destination, { force: true });
  if (
    lstatSync(source).isSymbolicLink() &&
    dirname(realpathSync(source)) === realpathSync(directory)
  ) {
    symlinkSync(basename(realpathSync(source)), destination);
  } else {
    copyFileSync(source, destination);
    chmodSync(destination, 0o755);
  }
}

export function installLinux(root: string, prefix: string, dataHome: string): void {
  for (const path of [prefix, dataHome]) {
    if (!path.startsWith("/") || /[\r\n\t]/.test(path)) {
      throw new Error("Install paths must be absolute and contain no tabs or line breaks.");
    }
  }
  const release = join(root, "src-tauri", "target", "release");
  for (const name of ["wtranscriber", "wt"]) {
    if (!statSync(join(release, name)).size) throw new Error(`Empty build output: ${name}`);
  }
  const lib = join(prefix, "lib", "wtranscriber");
  const bin = join(prefix, "bin");
  mkdirSync(lib, { recursive: true });
  const lock = join(lib, ".install-lock");
  try {
    mkdirSync(lock);
  } catch {
    throw new Error(`Install lock exists: ${lock}. Remove it only if no install is running.`);
  }
  let stage = "";
  let activated = false;
  try {
    stage = mkdtempSync(join(lib, "build-"));
    const manifest = join(release, ".wt-runtime.json");
    const runtime = existsSync(manifest)
      ? (JSON.parse(readFileSync(manifest, "utf8")) as { feature: string; libraries: string[] })
      : undefined;
    for (const name of readdirSync(release)) {
      if (
        ["wtranscriber", "wt"].includes(name) ||
        (!runtime && /\.so(?:\.|$)/.test(name) && name !== "libwtranscriber_lib.so")
      ) {
        copyLibrary(release, name, stage);
      }
    }
    for (const directory of runtime?.libraries ?? []) {
      for (const name of readdirSync(directory)) {
        if (/\.so(?:\.|$)/.test(name)) copyLibrary(directory, name, stage);
      }
    }
    const probe = spawnSync(join(stage, "wt"), ["--help"], {
      encoding: "utf8",
      env: {
        ...process.env,
        LD_LIBRARY_PATH: [stage, process.env.LD_LIBRARY_PATH].filter(Boolean).join(":"),
      },
    });
    if (probe.error || probe.status !== 0) {
      throw new Error(`Built CLI cannot run: ${probe.error?.message ?? probe.stderr}`);
    }
    const current = join(lib, "current");
    const previous = existsSync(current) ? readlinkSync(current) : "";
    mkdirSync(bin, { recursive: true });
    for (const name of ["wtranscriber", "wt"]) {
      const temporary = join(stage, `${name}.launcher`);
      writeFileSync(
        temporary,
        `#!/bin/sh\nexport LD_LIBRARY_PATH=${shellQuote(current)}\${LD_LIBRARY_PATH:+:"$LD_LIBRARY_PATH"}\nexec ${shellQuote(join(current, name))} "$@"\n`,
        { mode: 0o755 },
      );
      renameSync(temporary, join(bin, name));
    }
    const applications = join(dataHome, "applications");
    const icons = join(dataHome, "icons", "hicolor", "128x128", "apps");
    mkdirSync(applications, { recursive: true });
    mkdirSync(icons, { recursive: true });
    copyFileSync(join(root, "src-tauri", "icons", "128x128.png"), join(icons, "wtranscriber.png"));
    writeFileSync(
      join(applications, "com.asolopovas.wtranscriber.desktop"),
      `[Desktop Entry]\nType=Application\nName=WTranscriber\nExec=${desktopExec(join(bin, "wtranscriber"))}\nIcon=${desktopValue(join(icons, "wtranscriber.png"))}\nTerminal=false\nCategories=AudioVideo;Audio;\n`,
    );
    const next = join(lock, "current");
    symlinkSync(stage, next);
    renameSync(next, current);
    activated = true;
    if (dirname(previous) === lib && /^build-[\w-]+$/.test(basename(previous))) {
      rmSync(previous, { recursive: true, force: true });
    }
    console.log(`Installed WTranscriber and wt in ${bin}`);
    if (!(process.env.PATH ?? "").split(":").includes(bin)) {
      console.log(`Add to your shell profile: export PATH=${shellQuote(bin)}:"$PATH"`);
    }
  } finally {
    if (stage && !activated) rmSync(stage, { recursive: true, force: true });
    rmSync(lock, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  if (process.platform !== "linux") throw new Error("This installer requires Linux.");
  installLinux(
    resolve(import.meta.dir, ".."),
    process.env.WT_INSTALL_PREFIX ?? join(homedir(), ".local"),
    process.env.XDG_DATA_HOME ?? join(homedir(), ".local", "share"),
  );
}
