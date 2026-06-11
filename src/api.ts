import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { z } from "zod";
import {
  AudioMetaSchema,
  ConfigSchema,
  DirListingSchema,
  ExportFormatSchema,
  FileProgressSchema,
  ModelInfoSchema,
  ResetAppDataResultSchema,
  RuntimeProgressSchema,
  SuggestionSchema,
  SystemInfoSchema,
  TranscribeProgressSchema,
  TranscribeWarningSchema,
  TranscriptSchema,
} from "@/schemas";
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
  TranscribeWarning,
  Transcript,
} from "@/types";
import { utf8ToBase64 } from "@utils/base64";

type Schema<T> = z.ZodType<T>;

const StringSchema = z.string();
const NumberSchema = z.number();
const NullableNumberSchema = z.number().nullable();
const BooleanSchema = z.boolean();
const StringArraySchema = z.array(z.string());
const NumberArraySchema = z.array(z.number());
const ArrayBufferSchema = z.instanceof(ArrayBuffer);
const NullableTranscriptSchema = TranscriptSchema.nullable();

async function invokeParsed<T>(cmd: string, schema: Schema<T>, args?: InvokeArgs): Promise<T> {
  return schema.parse(await invoke<unknown>(cmd, args ?? {}));
}

function parsePayload<T>(schema: Schema<T>, payload: unknown): T {
  return schema.parse(payload);
}

export const api = {
  systemInfo: () => invokeParsed<SystemInfo>("system_info", SystemInfoSchema),
  loadConfig: () => invokeParsed<Config>("load_config", ConfigSchema),
  saveConfig: (config: Config) =>
    invoke<void>("save_config", { config: ConfigSchema.parse(config) }),
  listModels: () => invokeParsed<ModelInfo[]>("list_models", z.array(ModelInfoSchema)),
  essentialModels: () => invokeParsed<string[]>("essential_models", StringArraySchema),
  startEssentials: () => invoke<void>("start_essentials"),
  installModel: (id: string) => invoke<void>("install_model", { id }),
  probeAudio: (path: string) =>
    invokeParsed<number | null>("probe_audio", NullableNumberSchema, { path }),
  audioWaveform: (path: string, bins: number) =>
    invokeParsed<number[]>("audio_waveform", NumberArraySchema, { path, bins }),
  loadAudioMeta: (path: string) =>
    invokeParsed<AudioMeta>("load_audio_meta", AudioMetaSchema, { path }),
  saveAudioMeta: (path: string, meta: AudioMeta) =>
    invoke<void>("save_audio_meta", { path, meta: AudioMetaSchema.parse(meta) }),
  applyTrim: (path: string) =>
    invokeParsed<number | null>("apply_trim", NullableNumberSchema, { path }),
  transcribeFile: (input: string, config: Config) =>
    invokeParsed<Transcript>("transcribe_file", TranscriptSchema, {
      input,
      config: ConfigSchema.parse(config),
    }),
  redoDiarization: (input: string, oldCacheKey: string, config: Config) =>
    invokeParsed<Transcript>("redo_diarization", TranscriptSchema, {
      input,
      oldCacheKey,
      config: ConfigSchema.parse(config),
    }),
  cancelAllTranscribes: () => invokeParsed<number>("cancel_all_transcribes", NumberSchema),
  historyLoad: (key: string) =>
    invokeParsed<Transcript | null>("history_load", NullableTranscriptSchema, { key }),
  renameSpeaker: (key: string, old: string, name: string) =>
    invokeParsed<Transcript>("rename_speaker", TranscriptSchema, { key, old, new: name }),
  suggestFilename: (transcript: Transcript) =>
    invokeParsed<Suggestion>("suggest_filename", SuggestionSchema, {
      transcript: TranscriptSchema.parse(transcript),
    }),
  logTail: (maxBytes?: number) => invokeParsed<string>("log_tail", StringSchema, { maxBytes }),
  logClear: () => invoke<void>("log_clear"),
  resetTranscriptCache: () => invokeParsed<number>("reset_transcript_cache", NumberSchema),
  resetAudioCache: () => invokeParsed<number>("reset_audio_cache", NumberSchema),
  resetAppData: () => invokeParsed<ResetAppDataResult>("reset_app_data", ResetAppDataResultSchema),
  probeDuration: (path: string) =>
    invokeParsed<number | null>("probe_duration", NullableNumberSchema, { path }),
  listDirectory: (path?: string) =>
    invokeParsed<DirListing>("list_directory", DirListingSchema, { path }),
  defaultDir: () => invokeParsed<string>("default_dir", StringSchema),
  renameFile: (source: string, newName: string) =>
    invokeParsed<string>("rename_file", StringSchema, { source, newName }),
  deleteFile: (path: string) => invoke<void>("delete_file", { path }),
  revealInFolder: (path: string) => invoke<void>("reveal_in_folder", { path }),
  formatTranscript: (transcript: Transcript, format: ExportFormat) =>
    invokeParsed<string>("format_transcript", StringSchema, {
      transcript: TranscriptSchema.parse(transcript),
      format: ExportFormatSchema.parse(format),
    }),
  shareTranscript: (title: string, text: string) =>
    invokeParsed<boolean>("share_transcript", BooleanSchema, { title, text }),
  addToWorkdir: (source: string, workdir: string) =>
    invokeParsed<string>("add_to_workdir", StringSchema, { source, workdir }),
  saveRecording: async (workdir: string, filename: string, bytes: Uint8Array) =>
    StringSchema.parse(
      await invoke<unknown>("save_recording", bytes, {
        headers: {
          "x-workdir": utf8ToBase64(workdir),
          "x-filename": utf8ToBase64(filename),
        },
      }),
    ),
  readAudioBytes: (path: string) =>
    invokeParsed<ArrayBuffer>("read_audio_bytes", ArrayBufferSchema, { path }),
  logRenderer: (payload: {
    level: "error" | "warn" | "info";
    message: string;
    source?: string;
    line?: number;
    column?: number;
    stack?: string;
  }) => invoke<void>("log_renderer", payload),
  hasPersistentStorage: () => invokeParsed<boolean>("has_persistent_storage", BooleanSchema),
  requestPersistentStorage: () => invoke<void>("request_persistent_storage"),
  enablePersistentStorage: () => invokeParsed<boolean>("enable_persistent_storage", BooleanSchema),
  disablePersistentStorage: () => invoke<void>("disable_persistent_storage"),
};

export const events = {
  onModelProgress: (cb: (p: FileProgress) => void): Promise<UnlistenFn> =>
    listen<unknown>("model:progress", (e) => cb(parsePayload(FileProgressSchema, e.payload))),
  onModelDone: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<unknown>("model:done", (e) => cb(parsePayload(StringSchema, e.payload))),
  onModelError: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<unknown>("model:error", (e) => cb(parsePayload(StringSchema, e.payload))),
  onTranscribeProgress: (cb: (p: TranscribeProgress) => void): Promise<UnlistenFn> =>
    listen<unknown>("transcribe:progress", (e) =>
      cb(parsePayload(TranscribeProgressSchema, e.payload)),
    ),
  onTranscribeWarning: (cb: (w: TranscribeWarning) => void): Promise<UnlistenFn> =>
    listen<unknown>("transcribe:warning", (e) =>
      cb(parsePayload(TranscribeWarningSchema, e.payload)),
    ),
  onEssentialsDone: (cb: (ok: boolean) => void): Promise<UnlistenFn> =>
    listen<unknown>("model:essentials_done", (e) => cb(parsePayload(BooleanSchema, e.payload))),
  onRuntimeProgress: (cb: (p: RuntimeProgress) => void): Promise<UnlistenFn> =>
    listen<unknown>("runtime:progress", (e) => cb(parsePayload(RuntimeProgressSchema, e.payload))),
  onRuntimeDone: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<unknown>("runtime:done", (e) => cb(parsePayload(StringSchema, e.payload))),
  onRuntimeError: (cb: (id: string) => void): Promise<UnlistenFn> =>
    listen<unknown>("runtime:error", (e) => cb(parsePayload(StringSchema, e.payload))),
};
