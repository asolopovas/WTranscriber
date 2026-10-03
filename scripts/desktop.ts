import { spawn, spawnSync } from "node:child_process";
import {
  existsSync,
  cpSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join, resolve } from "node:path";

export interface CudaProbe {
  platform: string;
  arch: string;
  requested?: string;
  computeCapabilities: string[];
  nvccArchitectures: string[];
}

export function selectDesktopBackend(probe: CudaProbe): {
  feature: "cuda" | "sherpa-static";
  architectures: string[];
  reason: string;
} {
  if (![undefined, "", "auto", "0", "1"].includes(probe.requested)) {
    throw new Error("WT_CUDA must be auto, 0, or 1.");
  }
  const architectures = [
    ...new Set(probe.computeCapabilities.map((value) => value.replace(".", ""))),
  ].filter((value) => /^\d+$/.test(value) && probe.nvccArchitectures.includes(value));
  const reason =
    probe.requested === "0"
      ? "CPU requested with WT_CUDA=0"
      : probe.platform !== "linux" || probe.arch !== "x64"
        ? "automatic CUDA setup supports Linux x86_64"
        : probe.computeCapabilities.length === 0
          ? "no NVIDIA GPU is available"
          : probe.nvccArchitectures.length === 0
            ? "CUDA Toolkit nvcc is unavailable"
            : architectures.length === 0
              ? "the CUDA Toolkit does not support the detected GPU architecture"
              : "";
  if (probe.requested === "1" && reason) throw new Error(`CUDA requested but ${reason}.`);
  return reason
    ? { feature: "sherpa-static", architectures: [], reason }
    : { feature: "cuda", architectures, reason: "NVIDIA GPU and compatible CUDA Toolkit detected" };
}

function output(command: string, args: string[]): string {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 15_000 });
  return result.status === 0 ? result.stdout.trim() : "";
}

async function run(
  command: string,
  args: string[],
  options: { cwd: string; env?: NodeJS.ProcessEnv },
): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const child = spawn(command, args, { ...options, stdio: "inherit" });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) resolve();
      else reject(new Error(`${command} failed (${signal ?? `exit ${code}`}).`));
    });
  });
}

function hasLibraries(directory: string, libraries: string[]): boolean {
  return libraries.every((name) => existsSync(join(directory, "lib", name)));
}

async function ensureRuntime(
  root: string,
  cache: string,
  destination: string,
  url: string,
  libraries: string[],
): Promise<void> {
  if (hasLibraries(destination, libraries)) return;
  mkdirSync(cache, { recursive: true });
  mkdirSync(resolve(destination, ".."), { recursive: true });
  const archive = join(cache, new URL(url).pathname.split("/").at(-1)!);
  if (!existsSync(archive)) {
    const partial = `${archive}.partial-${process.pid}`;
    try {
      await run(
        "curl",
        ["--proto", "=https", "--tlsv1.2", "-fL", "--retry", "3", "-o", partial, url],
        { cwd: root },
      );
      renameSync(partial, archive);
    } finally {
      rmSync(partial, { force: true });
    }
  }
  const staging = mkdtempSync(join(cache, "extract-"));
  try {
    try {
      await run("tar", ["-xf", archive, "-C", staging], { cwd: root });
    } catch (error) {
      rmSync(archive, { force: true });
      throw error;
    }
    const extracted = [staging, ...readdirSync(staging).map((name) => join(staging, name))].find(
      (candidate) => hasLibraries(candidate, libraries),
    );
    if (!extracted) {
      rmSync(archive, { force: true });
      throw new Error(`Runtime archive is missing required libraries: ${url}`);
    }
    if (!hasLibraries(destination, libraries)) {
      rmSync(destination, { recursive: true, force: true });
      try {
        renameSync(extracted, destination);
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code !== "EXDEV") throw error;
        cpSync(extracted, destination, { recursive: true });
      }
    }
  } finally {
    rmSync(staging, { recursive: true, force: true });
  }
}

export function staleWhisperAbiCaches(
  root: string,
  cuda?: { architectures: string; compiler: string },
): boolean {
  if (process.platform !== "linux" || process.arch !== "x64") return false;
  return ["debug", "release"].some((profile) => {
    const build = join(root, "src-tauri", "target", profile, "build");
    if (!existsSync(build)) return false;
    return readdirSync(build).some((name) => {
      if (!name.startsWith("whisper-rs-sys-")) return false;
      const cache = join(build, name, "out", "build", "CMakeCache.txt");
      if (!existsSync(cache)) return false;
      const lines = readFileSync(cache, "utf8").split("\n");
      const value = (key: string) =>
        lines
          .find((line) => line.startsWith(`${key}:`))
          ?.split("=")
          .slice(1)
          .join("=");
      const flags = value("CMAKE_CXX_FLAGS");
      if (flags !== undefined && !flags.includes("_GLIBCXX_USE_CXX11_ABI=0")) return true;
      return (
        cuda !== undefined &&
        value("GGML_CUDA") === "ON" &&
        (value("CMAKE_CUDA_ARCHITECTURES") !== cuda.architectures ||
          value("CMAKE_CUDA_HOST_COMPILER") !== cuda.compiler)
      );
    });
  });
}

export function cudaHostCompiler(
  root: string,
  architecture: string,
  env: NodeJS.ProcessEnv,
): string {
  const explicit = env.CMAKE_CUDA_HOST_COMPILER || env.CUDAHOSTCXX;
  const candidates = explicit
    ? [explicit]
    : ["g++", "g++-14", "g++-13", "g++-12", "g++-11", "g++-10"];
  const probe = mkdtempSync(join(tmpdir(), "wt-nvcc-"));
  let failure = "no C++ compiler found";
  try {
    const source = join(probe, "probe.cu");
    writeFileSync(
      source,
      "#include <cuda_runtime.h>\n#include <cmath>\n__global__ void kernel() {}\n",
    );
    for (const candidate of candidates) {
      const compiler = candidate.includes("/")
        ? resolve(root, candidate)
        : (env.PATH ?? "")
            .split(":")
            .map((directory) => join(directory, candidate))
            .find(existsSync);
      if (!compiler || !output(compiler, ["--version"])) continue;
      const result = spawnSync(
        "nvcc",
        [
          "-ccbin",
          compiler,
          `-arch=sm_${architecture}`,
          "-c",
          source,
          "-o",
          join(probe, "probe.o"),
        ],
        {
          cwd: root,
          env,
          encoding: "utf8",
          timeout: 60_000,
        },
      );
      if (result.status === 0) return compiler;
      failure =
        result.error?.message || result.stderr.trim() || `${candidate} was rejected by nvcc`;
    }
  } finally {
    rmSync(probe, { recursive: true, force: true });
  }
  throw new Error(
    `CUDA Toolkit has no compatible host compiler. Install a supported g++ or set CMAKE_CUDA_HOST_COMPILER. ${failure}`,
  );
}

export async function desktop(mode: "dev" | "build", root: string): Promise<void> {
  const capabilities = output("nvidia-smi", ["--query-gpu=compute_cap", "--format=csv,noheader"])
    .split(/\r?\n/)
    .map((value) => value.trim())
    .filter(Boolean);
  const supported =
    output("nvcc", ["--list-gpu-arch"])
      .match(/compute_(\d+)/g)
      ?.map((value) => value.slice(8)) ?? [];
  const backend = selectDesktopBackend({
    platform: process.platform,
    arch: process.arch,
    requested: process.env.WT_CUDA,
    computeCapabilities: capabilities,
    nvccArchitectures: supported,
  });
  console.log(`desktop: ${backend.feature} (${backend.reason})`);
  const env = { ...process.env };
  const libraries: string[] = [];
  if (backend.feature === "cuda") {
    const data = join(
      env.XDG_DATA_HOME || join(homedir(), ".local", "share"),
      "wtranscriber",
      "third_party",
    );
    const cache = join(
      env.XDG_CACHE_HOME || join(homedir(), ".cache"),
      "wtranscriber",
      "desktop-setup",
    );
    const version = readFileSync(join(root, "src-tauri", "sherpa-version.txt"), "utf8").trim();
    const sherpa = join(data, "sherpa-onnx", `${version}-cuda`);
    const cudnn = join(data, "cudnn", "v9");
    const cudnnSource = readFileSync(
      join(root, "src-tauri", "src", "runtimes", "cudnn.rs"),
      "utf8",
    );
    const cudnnVersion = cudnnSource.match(/pub const VERSION: &str = "([\d.]+)"/)?.[1];
    if (!cudnnVersion) throw new Error("Cannot determine the application's cuDNN version.");
    await ensureRuntime(
      root,
      cache,
      sherpa,
      `https://github.com/k2-fsa/sherpa-onnx/releases/download/${version}/sherpa-onnx-${version}-cuda-12.x-cudnn-9.x-linux-x64-gpu.tar.bz2`,
      ["libsherpa-onnx-c-api.so", "libonnxruntime.so", "libonnxruntime_providers_cuda.so"],
    );
    await ensureRuntime(
      root,
      cache,
      cudnn,
      `https://developer.download.nvidia.com/compute/cudnn/redist/cudnn/linux-x86_64/cudnn-linux-x86_64-${cudnnVersion}_cuda12-archive.tar.xz`,
      ["libcudnn.so.9"],
    );
    env.CMAKE_CUDA_HOST_COMPILER = cudaHostCompiler(root, backend.architectures[0]!, env);
    console.log(`desktop: CUDA host compiler ${env.CMAKE_CUDA_HOST_COMPILER}`);
    libraries.push(join(sherpa, "lib"), join(cudnn, "lib"));
    env.SHERPA_ONNX_LIB_DIR = libraries[0];
    env.CMAKE_CUDA_ARCHITECTURES ||= backend.architectures.join(";");
    env.LD_LIBRARY_PATH = [...libraries, env.LD_LIBRARY_PATH].filter(Boolean).join(":");
  } else {
    delete env.SHERPA_ONNX_LIB_DIR;
    delete env.SHERPA_ONNX_LIB;
    delete env.SHERPA_ONNX_INCLUDE_DIR;
  }
  const cudaConfiguration =
    backend.feature === "cuda"
      ? { architectures: env.CMAKE_CUDA_ARCHITECTURES!, compiler: env.CMAKE_CUDA_HOST_COMPILER! }
      : undefined;
  if (staleWhisperAbiCaches(root, cudaConfiguration)) {
    console.log(
      "desktop: rebuilding cached Whisper objects for the current native compiler configuration",
    );
    await run(
      "cargo",
      ["clean", "--manifest-path", "src-tauri/Cargo.toml", "-p", "whisper-rs-sys"],
      { cwd: root, env },
    );
  }
  const args =
    mode === "dev"
      ? ["run", "tauri", "dev", "--no-default-features", "--features", backend.feature]
      : [
          "run",
          "tauri",
          "build",
          "--no-bundle",
          "--",
          "--locked",
          "--no-default-features",
          "--features",
          backend.feature,
        ];
  await run("bun", args, { cwd: root, env });
  if (mode === "build") {
    writeFileSync(
      join(root, "src-tauri", "target", "release", ".wt-runtime.json"),
      JSON.stringify({ feature: backend.feature, libraries }),
    );
  }
}

if (import.meta.main) {
  const mode = process.argv[2];
  if (mode !== "dev" && mode !== "build")
    throw new Error("Usage: bun scripts/desktop.ts dev|build");
  await desktop(mode, resolve(import.meta.dir, ".."));
}
