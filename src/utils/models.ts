import type { Config, DiarizerChoice, Engine, Family, ModelInfo, SystemInfo } from "@/types";

const ENGINES = new Set<string>(["whisper-onnx", "zipformer", "parakeet", "canary", "nemo-ctc"]);

export const DIARIZER_BY_MODEL_ID: Record<string, DiarizerChoice> = {
  "nemo-sortformer-v2": "nemo",
  "sherpa-pyannote-titanet": "titanet",
};

export function modelIdForDiarizer(choice: DiarizerChoice): string | null {
  for (const [id, value] of Object.entries(DIARIZER_BY_MODEL_ID)) {
    if (value === choice) return id;
  }
  return null;
}

export function isEngine(value: string): value is Engine {
  return ENGINES.has(value);
}

export function defaultModel(models: ModelInfo[], family: Family): ModelInfo | null {
  return (
    models.find((m) => m.family === family && m.default_active) ??
    models.find((m) => m.family === family) ??
    null
  );
}

export function applyMissingModelDefaults(config: Config, models: ModelInfo[]) {
  if (!config.llm_model) {
    const llm = defaultModel(models, "llm");
    if (llm) config.llm_model = llm.id;
  }
  if (!config.model) {
    const asr = defaultModel(models, "asr");
    if (asr) config.model = asr.id;
  }
}

export function applySystemConfigDefaults(config: Config, sys: SystemInfo | null) {
  if (sys && !sys.cuda_available && config.device === "cuda") config.device = "cpu";
  if (sys?.is_mobile && config.diarizer !== "titanet") config.diarizer = "titanet";
  const diarizer = config.diarizer as string;
  if (diarizer === "sherpa" || diarizer === "auto" || diarizer === "eres2net") {
    config.diarizer = sys?.is_mobile ? "titanet" : "nemo";
  }
}

export function applyAsrModel(config: Config, model: ModelInfo) {
  config.model = model.id;
  if (isEngine(model.engine)) config.engine = model.engine;
}

export function syncAsrEngineAndModel(config: Config, installedModels: ModelInfo[]) {
  if (!installedModels.length) return;
  const model = installedModels.find((m) => m.id === config.model);
  if (model) {
    if (isEngine(model.engine) && config.engine !== model.engine) config.engine = model.engine;
    return;
  }
  const engineModel = installedModels.find((m) => m.engine === config.engine);
  const fallback = engineModel ?? defaultModel(installedModels, "asr") ?? installedModels[0];
  applyAsrModel(config, fallback);
}
