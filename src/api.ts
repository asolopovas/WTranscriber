import { invoke } from "@tauri-apps/api/core";
import type { Config, ModelInfo, Utterance } from "./types";

export const api = {
  appVersion: () => invoke<string>("app_version"),
  loadConfig: () => invoke<Config>("load_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  transcribeFile: (input: string, config: Config) =>
    invoke<Utterance[]>("transcribe_file", { input, config }),
};
