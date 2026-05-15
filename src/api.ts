import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AudioMeta,
  Config,
  DirListing,
  ExportFormat,
  FileProgress,
  ModelInfo,
  ResetAppDataResult,
  RuntimeProgress,
  Suggestion,
  SystemInfo,
  TranscribeProgress,
  Transcript,
} from "@/types";
import { utf8ToBase64 } from "@utils/base64";

export const api = {
  systemInfo: () => invoke<SystemInfo>("system_info"),
  loadConfig: () => invoke<Config>("load_config"),
  saveConfig: (config: Config) => invoke<void>("save_config", { config }),
  listModels: () => invoke<ModelInfo[]>("list_models"),
  essentialModels: () => invoke<string[]>("essential_models"),
  startEssentials: () => invoke<void>("start_essentials"),
  installModel: (id: string) => invoke<void>("install_model", { id }),
  probeAudio: (path: string) => invoke<number | null>("probe_audio", { path }),
  audioWaveform: (path: string, bins: number) => invoke<number[]>("audio_waveform", { path, bins }),
  loadAudioMeta: (path: string) => invoke<AudioMeta>("load_audio_meta", { path }),
  saveAudioMeta: (path: string, meta: AudioMeta) => invoke<void>("save_audio_meta", { path, meta }),
  applyTrim: (path: string) => invoke<number | null>("apply_trim", { path }),
  transcribeFile: (input: string, config: Config) =>
    invoke<Transcript>("transcribe_file", { input, config }),
  redoDiarization: (input: string, oldCacheKey: string, config: Config) =>
    invoke<Transcript>("redo_diarization", { input, oldCacheKey, config }),
  cancelAllTranscribes: () => invoke<number>("cancel_all_transcribes"),
  historyLoad: (key: string) => invoke<Transcript | null>("history_load", { key }),
  renameSpeaker: (key: string, old: string, name: string) =>
    invoke<Transcript>("rename_speaker", { key, old, new: name }),
  suggestFilename: (transcript: Transcript) =>
    invoke<Suggestion>("suggest_filename", { transcript }),
  logTail: (maxBytes?: number) => invoke<string>("log_tail", { maxBytes }),
  logClear: () => invoke<void>("log_clear"),
  resetTranscriptCache: () => invoke<number>("reset_transcript_cache"),
  resetAudioCache: () => invoke<number>("reset_audio_cache"),
  resetAppData: () => invoke<ResetAppDataResult>("reset_app_data"),
  probeDuration: (path: string) => invoke<number | null>("probe_duration", { path }),
  listDirectory: (path?: string) => invoke<DirListing>("list_directory", { path }),
  defaultDir: () => invoke<string>("default_dir"),
  renameFile: (source: string, newName: string) =>
    invoke<string>("rename_file", { source, newName }),
  deleteFile: (path: string) => invoke<void>("delete_file", { path }),
  revealInFolder: (path: string) => invoke<void>("reveal_in_folder", { path }),
  formatTranscript: (transcript: Transcript, format: ExportFormat) =>
    invoke<string>("format_transcript", { transcript, format }),
  shareTranscript: (title: string, text: string) =>
    invoke<boolean>("share_transcript", { title, text }),
  addToWorkdir: (source: string, workdir: string) =>
    invoke<string>("add_to_workdir", { source, workdir }),
  saveRecording: (workdir: string, filename: string, bytes: Uint8Array) =>
    invoke<string>("save_recording", bytes, {
      headers: {
        "x-workdir": utf8ToBase64(workdir),
        "x-filename": utf8ToBase64(filename),
      },
    }),
  readAudioBytes: (path: string) => invoke<ArrayBuffer>("read_audio_bytes", { path }),
  logRenderer: (payload: {
    level: "error" | "warn" | "info";
    message: string;
    source?: string;
    line?: number;
    column?: number;
    stack?: string;
  }) => invoke<void>("log_renderer", payload),
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
  onRuntimeProgress: (cb: (p: RuntimeProgress) => void): Promise<UnlistenFn> =>
    listen<RuntimeProgress>("runtime:progress", (e) => cb(e.payload)),
  onRuntimeDone: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("runtime:done", (e) => cb(e.payload)),
  onRuntimeError: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<string>("runtime:error", (e) => cb(e.payload)),
};
