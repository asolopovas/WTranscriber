import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Config,
  DirListing,
  ExportFormat,
  FileProgress,
  ModelInfo,
  ModelStatus,
  Suggestion,
  TranscribeProgress,
  Transcript,
} from "./types";

export const api = {
  appVersion: () => invoke<string>("app_version"),
  loadConfig: () => invoke<Config>("load_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  modelStatus: (id: string) => invoke<ModelStatus>("model_status", { id }),
  installModel: (id: string) => invoke<void>("install_model", { id }),
  probeAudio: (path: string) => invoke<number | null>("probe_audio", { path }),
  audioWaveform: (path: string, bins: number) => invoke<number[]>("audio_waveform", { path, bins }),
  transcribeFile: (input: string, config: Config) =>
    invoke<Transcript>("transcribe_file", { input, config }),
  cancelTranscribe: (input: string) => invoke<boolean>("cancel_transcribe", { input }),
  historyLoad: (key: string) => invoke<Transcript | null>("history_load", { key }),
  suggestFilename: (transcript: Transcript) =>
    invoke<Suggestion>("suggest_filename", { transcript }),
  logPath: () => invoke<string>("log_path"),
  logTail: (maxBytes?: number) => invoke<string>("log_tail", { maxBytes }),
  logClear: () => invoke<void>("log_clear"),
  resetTranscriptCache: () => invoke<number>("reset_transcript_cache"),
  resetAudioCache: () => invoke<number>("reset_audio_cache"),
  listDirectory: (path?: string) => invoke<DirListing>("list_directory", { path }),
  defaultDir: () => invoke<string>("default_dir"),
  renameFile: (source: string, newName: string) =>
    invoke<string>("rename_file", { source, newName }),
  deleteFile: (path: string) => invoke<void>("delete_file", { path }),
  exportTranscript: (transcript: Transcript, dest: string, format: ExportFormat) =>
    invoke<string>("export_transcript", { transcript, dest, format }),
  addToWorkdir: (source: string, workdir: string) =>
    invoke<string>("add_to_workdir", { source, workdir }),
};

export const events = {
  onModelProgress: (cb: (p: FileProgress) => void): Promise<UnlistenFn> =>
    listen<FileProgress>("model:progress", (e) => cb(e.payload)),
  onModelDone: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("model:done", (e) => cb(e.payload)),
  onModelError: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("model:error", (e) => cb(e.payload)),
  onTranscribeProgress: (cb: (p: TranscribeProgress) => void): Promise<UnlistenFn> =>
    listen<TranscribeProgress>("transcribe:progress", (e) => cb(e.payload)),
};
