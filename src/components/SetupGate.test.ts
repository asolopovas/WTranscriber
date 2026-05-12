import { mount } from "@vue/test-utils";
import { describe, expect, it } from "vitest";
import type { FileProgress, ModelInfo } from "@/types";
import SetupGate from "./SetupGate.vue";

const model = (overrides: Partial<ModelInfo>): ModelInfo => ({
  id: "asr",
  family: "asr",
  engine: "whisper-onnx",
  display_name: "Speech model",
  description: "",
  size_bytes: 100 * 1024 * 1024,
  default_active: true,
  status: "not_installed",
  languages: [],
  ...overrides,
});

const progress = (overrides: Partial<FileProgress>): FileProgress => ({
  id: "asr",
  file_index: 0,
  file_count: 2,
  rel_path: "model.bin",
  downloaded: 25 * 1024 * 1024,
  total: 100 * 1024 * 1024,
  ...overrides,
});

describe("SetupGate", () => {
  it("renders an active runtime row above models", () => {
    const wrapper = mount(SetupGate, {
      props: {
        essentialIds: ["asr"],
        models: [model({ id: "asr" })],
        progress: {},
        errors: {},
        runtimes: {
          "sherpa-onnx-cuda": {
            id: "sherpa-onnx-cuda",
            downloaded: 350 * 1024 * 1024,
            total: 700 * 1024 * 1024,
            phase: "downloading",
            label: "Speech engine (sherpa-onnx CUDA)",
          },
        },
      },
    });
    const text = wrapper.text();
    expect(text).toContain("Installing Speech engine (sherpa-onnx CUDA)");
    expect(text).toContain("350 / 700 MB");
  });

  it("renders queued, downloading, installed, and error states", () => {
    const wrapper = mount(SetupGate, {
      props: {
        essentialIds: ["asr", "diarizer", "llm", "missing"],
        models: [
          model({ id: "asr", status: "downloading" }),
          model({
            id: "diarizer",
            family: "diarizer",
            display_name: "Diarizer",
            status: "installed",
          }),
          model({ id: "llm", family: "llm", display_name: "Namer" }),
        ],
        progress: { asr: progress({}) },
        errors: { missing: true },
        runtimes: {},
      },
    });

    const text = wrapper.text();

    expect(text).toContain("Downloading essentials");
    expect(text).toContain("Preparing speech, speakers and naming models.");
    expect(text).toContain("25 / 100 MB");
    expect(text).toContain("Speakers");
    expect(text).toContain("Ready");
    expect(text).toContain("Naming");
    expect(text).toContain("Queued");
    expect(text).toContain("Model");
    expect(text).toContain("Failed");
  });
});
