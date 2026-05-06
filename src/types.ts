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

export type ModelFamily = "whisper" | "parakeet" | "canary" | "nemo";

export interface ModelInfo {
  id: string;
  family: ModelFamily;
  installed: boolean;
  size_mb: number | null;
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
