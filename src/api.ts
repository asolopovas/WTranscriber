import { invoke } from "@tauri-apps/api/core";
import type { Config, ModelInfo, Transcript } from "./types";

export const api = {
  appVersion: () => invoke<string>("app_version"),
  loadConfig: () => invoke<Config>("load_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  probeAudio: (path: string) => invoke<number | null>("probe_audio", { path }),
  transcribeFile: (input: string, config: Config) =>
    invoke<Transcript>("transcribe_file", { input, config }),
};
