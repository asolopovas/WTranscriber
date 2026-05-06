export type Device = "cpu" | "cuda";

export interface Config {
  model: string;
  language: string;
  device: Device;
  threads: number;
  diarize: boolean;
  speakers: number | null;
  auto_rename: boolean;
}

export type Family = "asr" | "diarizer" | "llm";

export type ModelStatus = "not_installed" | "downloading" | "installed";

export interface ModelInfo {
  id: string;
  family: Family;
  display_name: string;
  description: string;
  size_bytes: number;
  default_active: boolean;
  status: ModelStatus;
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
}

export interface Word {
  text: string;
  start_ms: number;
  end_ms: number;
  speaker: string | null;
  confidence: number;
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
