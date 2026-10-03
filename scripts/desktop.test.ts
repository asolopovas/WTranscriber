import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { selectDesktopBackend, staleWhisperAbiCaches } from "./desktop";

const supported = {
  platform: "linux",
  arch: "x64",
  computeCapabilities: ["8.6"],
  nvccArchitectures: ["75", "80", "86", "89"],
};

describe("desktop CUDA selection", () => {
  it("enables CUDA for supported GPUs and limits compilation to installed architectures", () => {
    expect(selectDesktopBackend(supported)).toMatchObject({
      feature: "cuda",
      architectures: ["86"],
    });
    expect(
      selectDesktopBackend({ ...supported, computeCapabilities: ["8.6", "8.6", "8.9", "10.0"] }),
    ).toMatchObject({ feature: "cuda", architectures: ["86", "89"] });
  });

  it("uses a CPU build when no GPU or compatible toolkit is available", () => {
    expect(selectDesktopBackend({ ...supported, computeCapabilities: [] }).feature).toBe(
      "sherpa-static",
    );
    expect(selectDesktopBackend({ ...supported, nvccArchitectures: [] }).reason).toContain("nvcc");
    expect(selectDesktopBackend({ ...supported, nvccArchitectures: ["75"] }).feature).toBe(
      "sherpa-static",
    );
    expect(selectDesktopBackend({ ...supported, arch: "arm64" }).feature).toBe("sherpa-static");
  });

  it("honours CPU override and fails explicitly when forced CUDA cannot work", () => {
    expect(selectDesktopBackend({ ...supported, requested: "0" }).feature).toBe("sherpa-static");
    expect(() =>
      selectDesktopBackend({ ...supported, requested: "1", nvccArchitectures: [] }),
    ).toThrow("CUDA requested");
    expect(() => selectDesktopBackend({ ...supported, requested: "yes" })).toThrow("WT_CUDA");
  });
});

describe.skipIf(process.platform !== "linux" || process.arch !== "x64")(
  "native ABI cache migration",
  () => {
    let root = "";
    afterEach(() => {
      if (root) rmSync(root, { recursive: true, force: true });
    });

    it("rebuilds old cached Whisper objects once and leaves compatible caches intact", () => {
      root = mkdtempSync(join(tmpdir(), "wt-abi-test-"));
      expect(staleWhisperAbiCaches(root)).toBe(false);
      const cache = join(
        root,
        "src-tauri",
        "target",
        "debug",
        "build",
        "whisper-rs-sys-test",
        "out",
        "build",
      );
      mkdirSync(cache, { recursive: true });
      writeFileSync(join(cache, "CMakeCache.txt"), "CMAKE_CXX_FLAGS:STRING=-O3\n");
      expect(staleWhisperAbiCaches(root)).toBe(true);
      writeFileSync(
        join(cache, "CMakeCache.txt"),
        "CMAKE_CXX_FLAGS:STRING=-O3 -D_GLIBCXX_USE_CXX11_ABI=0\n",
      );
      expect(staleWhisperAbiCaches(root)).toBe(false);
      writeFileSync(
        join(cache, "CMakeCache.txt"),
        [
          "CMAKE_CXX_FLAGS:STRING=-D_GLIBCXX_USE_CXX11_ABI=0",
          "GGML_CUDA:BOOL=ON",
          "CMAKE_CUDA_ARCHITECTURES:UNINITIALIZED=86",
          "CMAKE_CUDA_HOST_COMPILER:UNINITIALIZED=/bin/g++-12",
        ].join("\n"),
      );
      expect(staleWhisperAbiCaches(root, { architectures: "86", compiler: "/bin/g++-12" })).toBe(
        false,
      );
      expect(staleWhisperAbiCaches(root, { architectures: "89", compiler: "/bin/g++-12" })).toBe(
        true,
      );
      expect(staleWhisperAbiCaches(root, { architectures: "86", compiler: "/bin/g++-13" })).toBe(
        true,
      );
    });
  },
);
