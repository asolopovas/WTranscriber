export type Device = "cpu" | "cuda";

export interface SystemInfo {
  os: string;
  arch: string;
  cpu_threads: number;
  is_mobile: boolean;
  cuda_available: boolean;
  nnapi_available: boolean;
  app_version: string;
  workdir: string | null;
  models_dir: string | null;
  cache_dir: string | null;
  config_dir: string | null;
  total_memory_bytes: number;
}

export type Engine = "whisper-onnx" | "zipformer" | "parakeet" | "canary" | "nemo-ctc";

export type DiarizerChoice = "auto" | "nemo" | "sherpa";

export interface Config {
  model: string;
  engine: Engine;
  language: string;
  device: Device;
  threads: number;
  diarize: boolean;
  speakers: number | null;
  diarizer: DiarizerChoice;
  auto_rename: boolean;
  last_dir?: string | null;
}

export type ExportFormat = "txt" | "csv" | "json" | "srt" | "vtt";

export interface DirEntry {
  name: string;
  path: string;
  is_dir: boolean;
  is_audio: boolean;
  size_bytes: number;
  modified_ms: number;
  cache_key: string | null;
  utterances: number | null;
  duration_ms: number | null;
  trim_start_ms: number | null;
  trim_end_ms: number | null;
}

export interface AudioMeta {
  trim_start_ms: number;
  trim_end_ms: number | null;
}

export interface DirListing {
  path: string;
  parent: string | null;
  entries: DirEntry[];
}

export type Family = "asr" | "diarizer" | "llm";

export type ModelStatus = "not_installed" | "downloading" | "installed";

export interface ModelInfo {
  id: string;
  family: Family;
  engine: string;
  display_name: string;
  description: string;
  size_bytes: number;
  default_active: boolean;
  status: ModelStatus;
  languages: string[];
}

export interface FileProgress {
  id: string;
  file_index: number;
  file_count: number;
  rel_path: string;
  downloaded: number;
  total: number;
}

export interface Utterance {
  start_ms: number;
  end_ms: number;
  speaker: string | null;
  text: string;
  language?: string;
}

export interface Word {
  text: string;
  start_ms: number;
  end_ms: number;
  speaker: string | null;
  confidence: number;
}

export interface Suggestion {
  topic: string;
  stamp: string;
}

export type TranscribePhase =
  | "cache_check"
  | "loading_audio"
  | "transcribing"
  | "diarizing"
  | "writing"
  | "done";

export interface TranscribeProgress {
  path: string;
  phase: TranscribePhase;
  displayPct: number;
  elapsedSec: number;
  etaSec: number;
}

export interface Transcript {
  model: string;
  language: string;
  duration_ms: number;
  diarizer?: string;
  device?: string;
  speakers_detected: number;
  utterances: Utterance[];
  words: Word[];
}
