import type { Page } from "@playwright/test";
import type { DirEntry } from "../../src/types";

declare global {
  interface Window {
    __TAURI_INTERNALS__: {
      metadata: { currentWindow: { label: string }; currentWebview: { label: string } };
      callbacks: Record<number, (event: unknown) => void>;
      transformCallback: (callback: (event: unknown) => void) => number;
      unregisterCallback: (id: number) => void;
      runCallback: (id: number, event: unknown) => void;
      convertFileSrc: (path: string) => string;
      invoke: (
        cmd: string,
        args?: Record<string, unknown>,
        options?: { headers?: Record<string, string> },
      ) => Promise<unknown>;
    };
    __WT_TEST__: {
      commandLog: string[];
      commandCalls: { command: string; args: Record<string, unknown> }[];
      setOpenPaths: (paths: string[]) => void;
      setModelInstalled: (id: string, installed: boolean) => void;
      clipboardText: string;
      recordingBytes: number[];
      savedConfigs: unknown[];
      emit: (event: string, payload: unknown) => void;
      finishTranscriptions: () => void;
      failTranscription: (input: string) => void;
      started: string[];
      transcriptionConfigs: unknown[];
    };
  }
}

const audioDir = "C:\\audio";

const audioFile = (name: string, overrides: Partial<DirEntry> = {}): DirEntry => ({
  name,
  path: `${audioDir}\\${name}`,
  is_dir: false,
  is_audio: true,
  size_bytes: 1_000,
  modified_ms: 1,
  cache_key: null,
  utterances: null,
  duration_ms: 1_000,
  trim_start_ms: null,
  trim_end_ms: null,
  ...overrides,
});

const files = [
  audioFile("board_meeting.wav", {
    size_bytes: 42_000_000,
    cache_key: "board",
    utterances: 2,
    duration_ms: 4_200_000,
  }),
  audioFile("interview.mp3", { size_bytes: 19_000_000, modified_ms: 2, duration_ms: 1_800_000 }),
  audioFile("field_notes.m4a", { size_bytes: 9_000_000, modified_ms: 3, duration_ms: 900_000 }),
];

const transcript = {
  model: "whisper-cpp-large-v3-turbo-q8",
  language: "en",
  duration_ms: 4_200_000,
  diarizer: "sortformer-onnx",
  device: "cuda",
  speakers_detected: 2,
  utterances: [
    { start_ms: 0, end_ms: 1_500, speaker: "SPEAKER_01", text: "Opening remarks." },
    { start_ms: 2_000, end_ms: 4_000, speaker: "SPEAKER_02", text: "Follow up answer." },
  ],
  words: [],
};

const config = {
  model: "whisper-cpp-large-v3-turbo-q8",
  engine: "whisper-cpp",
  language: "en",
  device: "cuda",
  threads: 8,
  diarize: true,
  speakers: null,
  diarizer: "sortformer-onnx",
  auto_rename: false,
  last_dir: audioDir,
  use_persistent_models: true,
  has_seen_persistent_prompt: true,
  debug_logging: false,
  precise_word_timestamps: true,
};

const models = [
  [
    "whisper-cpp-large-v3-turbo-q8",
    "asr",
    "whisper-cpp",
    "Whisper large-v3-turbo (ONNX, multilingual)",
    true,
  ],
  ["parakeet-tdt-0.6b-v3-int8", "asr", "parakeet", "Parakeet multilingual", false],
  [
    "sortformer-v2-onnx-4spk",
    "diarizer",
    "sortformer-onnx",
    "NVIDIA NeMo Sortformer 4-speaker v2",
    true,
  ],
  [
    "sherpa-pyannote-titanet",
    "diarizer",
    "sherpa",
    "pyannote-3.0 segmentation + TitaNet-Large",
    true,
  ],
].map(([id, family, engine, display_name, default_active]) => ({
  id,
  family,
  engine,
  display_name,
  description: `${display_name}`,
  size_bytes: 1_000,
  default_active,
  status: "installed",
  languages: ["en"],
}));

export async function installTauriMocks(page: Pick<Page, "addInitScript">) {
  await page.addInitScript(
    ({ seedFiles, seedTranscript, seedConfig, seedModels }) => {
      const callbacks: Record<number, (event: unknown) => void> = {};
      const listeners: Record<string, number[]> = {};
      const commandLog: string[] = [];
      const commandCalls: { command: string; args: Record<string, unknown> }[] = [];
      let openPaths: string[] | null = null;
      let logText = "transcribe ok\nsortformer-onnx\n";
      const savedConfigs: unknown[] = [];
      const audioMeta = new Map<
        string,
        { trim_start_ms: number; trim_end_ms: number | null; duration_ms: number | null }
      >();
      Object.defineProperty(navigator, "clipboard", {
        configurable: true,
        value: {
          writeText: async (text: string) => {
            window.__WT_TEST__.clipboardText = text;
          },
        },
      });

      function silentWav() {
        const data = new Uint8Array(60 * 16000 * 2 + 44);
        const view = new DataView(data.buffer);
        for (const [offset, text] of [
          [0, "RIFF"],
          [8, "WAVE"],
          [12, "fmt "],
          [36, "data"],
        ] as const) {
          for (let index = 0; index < text.length; index++)
            data[offset + index] = text.charCodeAt(index);
        }
        view.setUint32(4, data.length - 8, true);
        view.setUint32(16, 16, true);
        view.setUint16(20, 1, true);
        view.setUint16(22, 1, true);
        view.setUint32(24, 16000, true);
        view.setUint32(28, 32000, true);
        view.setUint16(32, 2, true);
        view.setUint16(34, 16, true);
        view.setUint32(40, data.length - 44, true);
        return data.buffer;
      }
      const rows = seedFiles.map((file) => ({ ...file }));
      const cancelled = new Set<string>();
      const pending = new Map<string, () => void>();
      const failed = new Set<string>();
      const started: string[] = [];
      const transcriptionConfigs: unknown[] = [];
      let nextCallback = 1;
      let nextEvent = 1;

      function emit(event: string, payload: unknown) {
        for (const id of listeners[event] ?? []) callbacks[id]?.({ event, payload });
      }

      function finishTranscriptions() {
        for (const done of pending.values()) done();
        pending.clear();
      }

      function markTranscribed(input: string) {
        const row = rows.find((file) => file.path === input);
        if (!row) return;
        row.cache_key = row.name.replace(/\.[^.]+$/, "");
        row.utterances = seedTranscript.utterances.length;
        row.duration_ms = seedTranscript.duration_ms;
      }

      const replies: Record<string, (args: Record<string, unknown>) => unknown> = {
        app_version: () => "0.1.0",
        system_info: () => ({
          os: "windows",
          arch: "x86_64",
          cpu_threads: 8,
          is_mobile: false,
          cuda_available: true,
          nnapi_available: false,
          app_version: "0.1.0",
          workdir: "C:\\audio",
          models_dir: "C:\\models",
          cache_dir: "C:\\cache",
          config_dir: "C:\\config",
          total_memory_bytes: 16_000_000_000,
        }),
        load_config: () => ({ ...seedConfig }),
        save_config: (args) => savedConfigs.push(args.config) && null,
        list_models: () => structuredClone(seedModels),
        install_model: ({ id }) => {
          const model = seedModels.find((model) => model.id === id);
          if (model) model.status = "installed";
          emit("model:done", id);
          return null;
        },
        essential_models: () => [],
        start_essentials: () => null,
        audio_waveform: () => Array.from({ length: 160 }, (_, i) => 0.15 + ((i * 17) % 80) / 100),
        default_dir: () => "C:\\audio",
        list_directory: () => ({
          path: "C:\\audio",
          parent: null,
          entries: rows.map((row) => ({ ...row })),
        }),
        history_load: () => structuredClone(seedTranscript),
        rename_speaker: (args) => {
          for (const utterance of seedTranscript.utterances) {
            if (utterance.speaker === args.old) utterance.speaker = String(args.new);
          }
          return structuredClone(seedTranscript);
        },
        read_audio_bytes: () => silentWav(),
        probe_audio: () => 60000,
        load_audio_meta: ({ path }) =>
          audioMeta.get(String(path)) ?? { trim_start_ms: 0, trim_end_ms: null, duration_ms: null },
        save_audio_meta: ({ path, meta }) => {
          const value = meta as {
            trim_start_ms: number;
            trim_end_ms: number | null;
            duration_ms: number | null;
          };
          audioMeta.set(String(path), value);
          const row = rows.find((row) => row.path === path);
          if (row) {
            row.trim_start_ms = value.trim_start_ms;
            row.trim_end_ms = value.trim_end_ms;
          }
          return null;
        },
        suggest_filename: () => ({ topic: "meeting_notes", stamp: "20260506" }),
        rename_file: ({ source, newName }) => {
          const row = rows.find((row) => row.path === source);
          if (!row) throw "file not found";
          row.name = String(newName);
          row.path = `C:\\audio\\${newName}`;
          return row.path;
        },
        delete_file: ({ path }) => {
          const index = rows.findIndex((row) => row.path === path);
          if (index >= 0) rows.splice(index, 1);
          return null;
        },
        reveal_in_folder: () => null,
        probe_duration: () => 60000,
        format_transcript: ({ format }) => `${format}: Opening remarks.\nFollow up answer.`,
        "plugin:fs|write_text_file": () => null,
        export_transcript: (args) => args.dest,
        add_to_workdir: ({ source }) => {
          const name = String(source).split(/[\\/]/).pop()!;
          const path = `C:\\audio\\${name}`;
          if (!rows.some((row) => row.path === path)) {
            rows.push({ ...seedFiles[1], name, path, cache_key: null, duration_ms: 60000 });
          }
          return path;
        },
        log_path: () => "C:\\logs\\wt.log",
        log_tail: () => logText,
        log_clear: () => {
          logText = "";
          return null;
        },
        reset_transcript_cache: () => {
          for (const row of rows) {
            row.cache_key = null;
            row.utterances = null;
          }
          return rows.length;
        },
        reset_audio_cache: () => 2,
        reset_app_data: () => {
          const count = rows.length;
          rows.splice(0);
          return { cache_entries_removed: 3, workdir_entries_removed: count };
        },
        "plugin:event|listen": (args) => {
          const event = args.event as string;
          listeners[event] = [...(listeners[event] ?? []), args.handler as number];
          return nextEvent++;
        },
        "plugin:event|unlisten": () => null,
        "plugin:dialog|open": () => {
          const selected = openPaths;
          openPaths = null;
          return selected;
        },
        "plugin:dialog|save": () => "C:\\exports\\board.txt",
        "plugin:dialog|message": ({ buttons }) => {
          if (typeof buttons === "object" && buttons !== null && "OkCancelCustom" in buttons) {
            return (buttons.OkCancelCustom as string[])[0];
          }
          return "Ok";
        },
        "plugin:dialog|confirm": () => true,
      };

      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
        callbacks,
        transformCallback(callback: (event: unknown) => void) {
          const id = nextCallback++;
          callbacks[id] = callback;
          return id;
        },
        unregisterCallback: (id: number) => delete callbacks[id],
        runCallback: (id: number, event: unknown) => callbacks[id]?.(event),
        convertFileSrc: (path: string) => `asset://localhost/${encodeURIComponent(path)}`,
        async invoke(
          cmd: string,
          args: Record<string, unknown> = {},
          options?: { headers?: Record<string, string> },
        ) {
          commandLog.push(cmd);
          commandCalls.push({ command: cmd, args: structuredClone(args) });
          if (cmd === "save_recording") {
            window.__WT_TEST__.recordingBytes = Array.from(args as unknown as Uint8Array);
            const filename = atob(options?.headers?.["x-filename"] ?? "");
            const path = `C:\\audio\\${filename}`;
            rows.push({ ...seedFiles[1], name: filename, path, cache_key: null, duration_ms: 10 });
            return path;
          }
          if (cmd === "transcribe_file" || cmd === "redo_diarization") {
            const input = args.input as string;
            started.push(input);
            transcriptionConfigs.push(args.config);
            cancelled.delete(input);
            emit("transcribe:progress", {
              path: input,
              phase: cmd === "redo_diarization" ? "diarizing" : "transcribing",
              displayPct: 12,
              elapsedSec: 2,
              totalSec: 12,
            });
            await new Promise<void>((resolve) => pending.set(input, resolve));
            pending.delete(input);
            if (cancelled.has(input)) throw "cancelled";
            if (failed.delete(input)) throw "test transcription failed";
            markTranscribed(input);
            return {
              ...seedTranscript,
              duration_ms: rows.find((file) => file.path === input)?.duration_ms ?? 0,
            };
          }
          if (cmd === "cancel_transcribe") {
            const input = args.input as string;
            if (!pending.has(input)) return false;
            cancelled.add(input);
            pending.get(input)?.();
            return true;
          }
          if (cmd === "cancel_all_transcribes") {
            for (const input of pending.keys()) cancelled.add(input);
            finishTranscriptions();
            return cancelled.size;
          }
          const reply = replies[cmd];
          if (reply) return reply(args);
          throw new Error(`unhandled invoke ${cmd}`);
        },
      };
      window.__WT_TEST__ = {
        commandLog,
        commandCalls,
        clipboardText: "",
        recordingBytes: [],
        setModelInstalled(id, installed) {
          const model = seedModels.find((model) => model.id === id);
          if (model) model.status = installed ? "installed" : "not_installed";
          emit("model:done", id);
        },
        setOpenPaths(paths) {
          openPaths = paths;
        },
        savedConfigs,
        emit,
        finishTranscriptions,
        started,
        transcriptionConfigs,
        failTranscription(input) {
          failed.add(input);
          pending.get(input)?.();
        },
      };
    },
    { seedFiles: files, seedTranscript: transcript, seedConfig: config, seedModels: models },
  );
}
