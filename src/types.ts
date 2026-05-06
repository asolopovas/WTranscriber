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
