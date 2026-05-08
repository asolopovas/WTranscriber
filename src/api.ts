import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AudioMeta,
  Config,
  DirListing,
  ExportFormat,
  FileProgress,
  ModelInfo,
  ModelStatus,
  Suggestion,
  SystemInfo,
  TranscribeProgress,
  Transcript,
} from "@/types";

export const api = {
  appVersion: () => invoke<string>("app_version"),
  systemInfo: () => invoke<SystemInfo>("system_info"),
  loadConfig: () => invoke<Config>("load_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  essentialModels: () => invoke<string[]>("essential_models"),
  modelStatus: (id: string) => invoke<ModelStatus>("model_status", { id }),
  installModel: (id: string) => invoke<void>("install_model", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  probeAudio: (path: string) => invoke<number | null>("probe_audio", { path }),
  audioWaveform: (path: string, bins: number) => invoke<number[]>("audio_waveform", { path, bins }),
  loadAudioMeta: (path: string) => invoke<AudioMeta>("load_audio_meta", { path }),
  saveAudioMeta: (path: string, meta: AudioMeta) => invoke<void>("save_audio_meta", { path, meta }),
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
  saveRecording: (workdir: string, filename: string, bytes: Uint8Array) =>
    invoke<string>("save_recording", { workdir, filename, bytes: Array.from(bytes) }),
  readAudioBytes: (path: string) => invoke<number[]>("read_audio_bytes", { path }),
  hasPersistentStorage: () => invoke<boolean>("has_persistent_storage"),
  requestPersistentStorage: () => invoke<void>("request_persistent_storage"),
  enablePersistentStorage: () => invoke<boolean>("enable_persistent_storage"),
  disablePersistentStorage: () => invoke<void>("disable_persistent_storage"),
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
  onEssentialsDone: (cb: (ok: boolean) => void): Promise<UnlistenFn> =>
    listen<boolean>("model:essentials_done", (e) => cb(e.payload)),
};
