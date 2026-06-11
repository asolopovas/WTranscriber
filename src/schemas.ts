import { z } from "zod";
import type {
  AudioMeta,
  Config,
  DirEntry,
  DirListing,
  ExportFormat,
  FileProgress,
  ModelInfo,
  ResetAppDataResult,
  RuntimeProgress,
  Suggestion,
  SystemInfo,
  TranscribePhase,
  TranscribeProgress,
  TranscribeWarning,
  Transcript,
  Utterance,
  Word,
} from "@/types";

export const DeviceSchema = z.enum(["cpu", "cuda"]);
export const EngineSchema = z.enum(["parakeet", "nemo-ctc", "whisper-cpp"]);
export const DiarizerChoiceSchema = z.enum(["sortformer-onnx", "titanet"]);
export const ExportFormatSchema: z.ZodType<ExportFormat> = z.enum([
  "txt",
  "csv",
  "json",
  "srt",
  "vtt",
]);

export const SystemInfoSchema: z.ZodType<SystemInfo> = z.object({
  os: z.string(),
  arch: z.string(),
  cpu_threads: z.number(),
  is_mobile: z.boolean(),
  cuda_available: z.boolean(),
  nnapi_available: z.boolean(),
  app_version: z.string(),
  workdir: z.string().nullable(),
  models_dir: z.string().nullable(),
  cache_dir: z.string().nullable(),
  config_dir: z.string().nullable(),
  total_memory_bytes: z.number(),
});

export const ConfigSchema: z.ZodType<Config> = z.object({
  model: z.string(),
  engine: EngineSchema,
  language: z.string(),
  device: DeviceSchema,
  threads: z.number(),
  diarize: z.boolean(),
  speakers: z.number().nullable(),
  diarizer: DiarizerChoiceSchema,
  auto_rename: z.boolean(),
  llm_model: z.string().nullable().optional(),
  last_dir: z.string().nullable().optional(),
  use_persistent_models: z.boolean(),
  has_seen_persistent_prompt: z.boolean(),
  debug_logging: z.boolean(),
  precise_word_timestamps: z.boolean(),
});

export const DirEntrySchema: z.ZodType<DirEntry> = z.object({
  name: z.string(),
  path: z.string(),
  is_dir: z.boolean(),
  is_audio: z.boolean(),
  size_bytes: z.number(),
  modified_ms: z.number(),
  cache_key: z.string().nullable(),
  utterances: z.number().nullable(),
  duration_ms: z.number().nullable(),
  trim_start_ms: z.number().nullable(),
  trim_end_ms: z.number().nullable(),
});

export const AudioMetaSchema: z.ZodType<AudioMeta> = z.object({
  trim_start_ms: z.number(),
  trim_end_ms: z.number().nullable(),
  duration_ms: z.number().nullable(),
});

export const ResetAppDataResultSchema: z.ZodType<ResetAppDataResult> = z.object({
  cache_entries_removed: z.number(),
  workdir_entries_removed: z.number(),
});

export const DirListingSchema: z.ZodType<DirListing> = z.object({
  path: z.string(),
  parent: z.string().nullable(),
  entries: z.array(DirEntrySchema),
});

export const TranscribeWarningSchema: z.ZodType<TranscribeWarning> = z.object({
  path: z.string(),
  message: z.string(),
});

export const FamilySchema = z.enum(["asr", "diarizer", "llm", "langid"]);
export const ModelStatusSchema = z.enum(["not_installed", "downloading", "installed"]);

export const ModelInfoSchema: z.ZodType<ModelInfo> = z.object({
  id: z.string(),
  family: FamilySchema,
  engine: z.string(),
  display_name: z.string(),
  description: z.string(),
  size_bytes: z.number(),
  default_active: z.boolean(),
  status: ModelStatusSchema,
  languages: z.array(z.string()),
});

export const FileProgressSchema: z.ZodType<FileProgress> = z.object({
  id: z.string(),
  file_index: z.number(),
  file_count: z.number(),
  rel_path: z.string(),
  downloaded: z.number(),
  total: z.number(),
});

export const RuntimeProgressSchema: z.ZodType<RuntimeProgress> = z.object({
  id: z.string(),
  downloaded: z.number().optional(),
  total: z.number().optional(),
  phase: z.string().optional(),
  line: z.string().optional(),
});

export const UtteranceSchema: z.ZodType<Utterance> = z.object({
  start_ms: z.number(),
  end_ms: z.number(),
  speaker: z.string().nullable(),
  text: z.string(),
  language: z.string().optional(),
});

export const WordSchema: z.ZodType<Word> = z.object({
  text: z.string(),
  start_ms: z.number(),
  end_ms: z.number(),
  speaker: z.string().nullable(),
  confidence: z.number(),
});

export const SuggestionSchema: z.ZodType<Suggestion> = z.object({
  topic: z.string(),
  stamp: z.string(),
});

export const TranscribePhaseSchema: z.ZodType<TranscribePhase> = z.enum([
  "cache_check",
  "loading_audio",
  "transcribing",
  "diarizing",
  "writing",
  "done",
]);

export const TranscribeProgressSchema: z.ZodType<TranscribeProgress> = z.object({
  path: z.string(),
  phase: TranscribePhaseSchema,
  displayPct: z.number(),
  elapsedSec: z.number(),
  totalSec: z.number(),
});

export const TranscriptSchema: z.ZodType<Transcript> = z.object({
  model: z.string(),
  language: z.string(),
  duration_ms: z.number(),
  diarizer: z.string().optional(),
  device: z.string().optional(),
  speakers_detected: z.number(),
  utterances: z.array(UtteranceSchema),
  words: z.array(WordSchema),
});
