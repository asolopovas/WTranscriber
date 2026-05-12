import { describe, expect, it } from "vitest";
import type { Config, ModelInfo } from "@/types";
import {
  applyAsrModel,
  applyMissingModelDefaults,
  applySystemConfigDefaults,
  availableDiarizerOptions,
  defaultModel,
  diarizerSpeakerCap,
  modelIdForDiarizer,
  speakerOptionsForDiarizer,
  syncAsrEngineAndModel,
} from "./models";

const model = (overrides: Partial<ModelInfo>): ModelInfo => ({
  id: "id",
  family: "asr",
  engine: "parakeet",
  display_name: "Model",
  description: "",
  size_bytes: 1,
  default_active: false,
  status: "installed",
  languages: [],
  ...overrides,
});

const config = (): Config => ({
  model: "",
  engine: "parakeet",
  language: "auto",
  device: "cuda",
  threads: 4,
  diarize: true,
  speakers: null,
  diarizer: "sortformer-onnx",
  auto_rename: false,
  llm_model: null,
  last_dir: null,
  use_persistent_models: false,
});

describe("model helpers", () => {
  it("selects active defaults by family", () => {
    const fallback = model({ id: "fallback", family: "llm" });
    const active = model({ id: "active", family: "llm", default_active: true });

    expect(defaultModel([fallback, active], "llm")?.id).toBe("active");
  });

  it("applies missing ASR and LLM defaults", () => {
    const cfg = config();

    applyMissingModelDefaults(cfg, [
      model({ id: "asr-default", family: "asr", default_active: true }),
      model({ id: "llm-default", family: "llm", default_active: true }),
    ]);

    expect(cfg.model).toBe("asr-default");
    expect(cfg.llm_model).toBe("llm-default");
  });

  it("downgrades cuda to cpu when system reports no cuda", () => {
    const cfg = config();

    applySystemConfigDefaults(cfg, {
      os: "linux",
      arch: "x86_64",
      cpu_threads: 16,
      is_mobile: false,
      cuda_available: false,
      nnapi_available: false,
      app_version: "test",
      workdir: null,
      models_dir: null,
      cache_dir: null,
      config_dir: null,
      total_memory_bytes: 1,
    });

    expect(cfg.device).toBe("cpu");
  });

  it("maps diarizer choices to model ids", () => {
    expect(modelIdForDiarizer("sortformer-onnx")).toBe("sortformer-v2-onnx-4spk");
    expect(modelIdForDiarizer("titanet")).toBe("sherpa-pyannote-titanet");
  });

  it("exposes the same diarizer options on every platform", () => {
    const values = ["sortformer-onnx", "titanet"];
    expect(availableDiarizerOptions(false).map((o) => o.value)).toEqual(values);
    expect(availableDiarizerOptions(true).map((o) => o.value)).toEqual(values);
  });

  it("builds speaker options for each diarizer cap", () => {
    expect(diarizerSpeakerCap("sortformer-onnx")).toBe(4);
    expect(diarizerSpeakerCap("titanet")).toBe(10);
    const titanetOptions = speakerOptionsForDiarizer("titanet");
    expect(speakerOptionsForDiarizer("sortformer-onnx")).toHaveLength(5);
    expect(titanetOptions[titanetOptions.length - 1]).toEqual({ value: 10, label: "10" });
  });

  it("applies ASR model only when engine is known", () => {
    const cfg = config();

    applyAsrModel(cfg, model({ id: "known", engine: "whisper-cpp" }));
    expect(cfg).toMatchObject({ model: "known", engine: "whisper-cpp" });

    applyAsrModel(cfg, model({ id: "future", engine: "future-engine" }));
    expect(cfg).toMatchObject({ model: "future", engine: "whisper-cpp" });
  });

  it("syncs missing selected ASR model to an installed fallback", () => {
    const cfg = config();
    cfg.model = "missing";

    syncAsrEngineAndModel(cfg, [
      model({ id: "fallback", engine: "whisper-cpp", default_active: true }),
    ]);

    expect(cfg).toMatchObject({ model: "fallback", engine: "whisper-cpp" });
  });
});
