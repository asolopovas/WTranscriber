import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { Config, FileProgress, ModelInfo, ModelStatus, Transcript } from "./types";

export const api = {
  appVersion: () => invoke<string>("app_version"),
  loadConfig: () => invoke<Config>("load_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  modelStatus: (id: string) => invoke<ModelStatus>("model_status", { id }),
  installModel: (id: string) => invoke<void>("install_model", { id }),
  probeAudio: (path: string) => invoke<number | null>("probe_audio", { path }),
  transcribeFile: (input: string, config: Config) =>
    invoke<Transcript>("transcribe_file", { input, config }),
};

export const events = {
  onModelProgress: (cb: (p: FileProgress) => void): Promise<UnlistenFn> =>
    listen<FileProgress>("model:progress", (e) => cb(e.payload)),
  onModelDone: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("model:done", (e) => cb(e.payload)),
  onModelError: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("model:error", (e) => cb(e.payload)),
};
