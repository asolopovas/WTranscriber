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
      },
    });

    const text = wrapper.text();

    expect(text).toContain("Speech model");
    expect(text).toContain("25 / 100 MB");
    expect(text).toContain("Diarizer");
    expect(text).toContain("ready");
    expect(text).toContain("Namer");
    expect(text).toContain("queued");
    expect(text).toContain("missing");
    expect(text).toContain("failed");
  });
});
