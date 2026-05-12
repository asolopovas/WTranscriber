import type { Config, DiarizerChoice, Engine, Family, ModelInfo, SystemInfo } from "@/types";

const ENGINES = new Set<string>(["parakeet", "nemo-ctc", "whisper-cpp"]);

export const DIARIZER_BY_MODEL_ID: Record<string, DiarizerChoice> = {
  "sortformer-v2-onnx-4spk": "sortformer-onnx",
  "sherpa-pyannote-titanet": "titanet",
};

export interface DiarizerOption {
  value: DiarizerChoice;
  label: string;
  desktopOnly?: boolean;
}

export const DIARIZER_OPTIONS: readonly DiarizerOption[] = [
  {
    value: "sortformer-onnx",
    label: "NVIDIA Sortformer v2.1 (ONNX, ≤4 speakers)",
  },
  { value: "titanet", label: "pyannote-3.0 + TitaNet-Large (>4 speakers)" },
];

export function availableDiarizerOptions(isMobile: boolean): readonly DiarizerOption[] {
  return isMobile ? DIARIZER_OPTIONS.filter((option) => !option.desktopOnly) : DIARIZER_OPTIONS;
}

export function diarizerSpeakerCap(choice: DiarizerChoice): number {
  return choice === "sortformer-onnx" ? 4 : 10;
}

export function speakerOptionsForDiarizer(
  choice: DiarizerChoice,
): { value: number; label: string }[] {
  return [
    { value: 0, label: "Auto" },
    ...Array.from({ length: diarizerSpeakerCap(choice) }, (_, i) => {
      const value = i + 1;
      return { value, label: String(value) };
    }),
  ];
}

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
