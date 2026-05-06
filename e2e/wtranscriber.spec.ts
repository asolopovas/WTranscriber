import { expect, test, type Page } from "@playwright/test";

declare global {
  interface Window {
    __TAURI_INTERNALS__: {
      metadata: { currentWindow: { label: string }; currentWebview: { label: string } };
      callbacks: Record<number, (event: unknown) => void>;
      transformCallback: (callback: (event: unknown) => void) => number;
      unregisterCallback: (id: number) => void;
      runCallback: (id: number, event: unknown) => void;
      convertFileSrc: (path: string) => string;
      invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
    };
    __WT_TEST__: {
      commandLog: string[];
      emit: (event: string, payload: unknown) => void;
    };
  }
}

const files = [
  {
    name: "board_meeting.wav",
    path: "C:\\audio\\board_meeting.wav",
    is_dir: false,
    is_audio: true,
    size_bytes: 42_000_000,
    modified_ms: 1,
    cache_key: "board",
    utterances: 2,
    duration_ms: 4_200_000,
  },
  {
    name: "interview.mp3",
    path: "C:\\audio\\interview.mp3",
    is_dir: false,
    is_audio: true,
    size_bytes: 19_000_000,
    modified_ms: 2,
    cache_key: null,
    utterances: null,
    duration_ms: 1_800_000,
  },
  {
    name: "field_notes.m4a",
    path: "C:\\audio\\field_notes.m4a",
    is_dir: false,
    is_audio: true,
    size_bytes: 9_000_000,
    modified_ms: 3,
    cache_key: null,
    utterances: null,
    duration_ms: 900_000,
  },
];

const transcript = {
  model: "sherpa-whisper-turbo",
  language: "en",
  duration_ms: 4_200_000,
  diarizer: "nemo-sortformer",
  device: "cuda",
  speakers_detected: 2,
  utterances: [
    { start_ms: 0, end_ms: 1_500, speaker: "SPEAKER_01", text: "Opening remarks." },
    { start_ms: 2_000, end_ms: 4_000, speaker: "SPEAKER_02", text: "Follow up answer." },
  ],
  words: [],
};

async function installTauriMocks(page: Page) {
  await page.addInitScript(
    ({ seedFiles, seedTranscript }) => {
      const callbacks: Record<number, (event: unknown) => void> = {};
      const listeners: Record<string, number[]> = {};
      const commandLog: string[] = [];
      const rows = seedFiles.map((f) => ({ ...f }));
      const cancelled = new Set<string>();
      let nextCallback = 1;
      let nextEvent = 1;

      function emit(event: string, payload: unknown) {
        for (const id of listeners[event] ?? []) {
          callbacks[id]?.({ event, payload });
        }
      }

      function markTranscribed(input: string) {
        const row = rows.find((f) => f.path === input);
        if (row) {
          row.cache_key = row.name.replace(/\.[^.]+$/, "");
          row.utterances = seedTranscript.utterances.length;
          row.duration_ms = seedTranscript.duration_ms;
        }
      }

      window.__TAURI_INTERNALS__ = {
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
        callbacks,
        transformCallback(callback: (event: unknown) => void) {
          const id = nextCallback++;
          callbacks[id] = callback;
          return id;
        },
        unregisterCallback(id: number) {
          delete callbacks[id];
        },
        runCallback(id: number, event: unknown) {
          callbacks[id]?.(event);
        },
        convertFileSrc(path: string) {
          return `asset://localhost/${encodeURIComponent(path)}`;
        },
        async invoke(cmd: string, args: Record<string, unknown> = {}) {
          commandLog.push(cmd);
          if (cmd === "app_version") return "0.1.0";
          if (cmd === "load_config") {
            return {
              model: "sherpa-whisper-turbo",
              engine: "whisper-onnx",
              language: "en",
              device: "cuda",
              threads: 8,
              diarize: true,
              speakers: null,
              auto_rename: false,
              last_dir: "C:\\audio",
            };
          }
          if (cmd === "save_config") return null;
          if (cmd === "list_models") {
            return [
              {
                id: "sherpa-whisper-turbo",
                family: "asr",
                display_name: "Whisper large-v3-turbo (ONNX, multilingual)",
                description: "Best default ASR model",
                size_bytes: 1_036_613_791,
                default_active: true,
                status: "installed",
              },
              {
                id: "nemo-sortformer-v2",
                family: "diarizer",
                display_name: "NVIDIA NeMo Sortformer 4-speaker v2",
                description: "GPU-first NVIDIA NeMo diarization",
                size_bytes: 0,
                default_active: true,
                status: "installed",
              },
              {
                id: "sherpa-pyannote-titanet",
                family: "diarizer",
                display_name: "pyannote-3.0 segmentation + TitaNet-Large",
                description: "Fallback diarizer",
                size_bytes: 107_398_406,
                default_active: true,
                status: "installed",
              },
            ];
          }
          if (cmd === "default_dir") return "C:\\audio";
          if (cmd === "list_directory") return { path: "C:\\audio", parent: null, entries: rows };
          if (cmd === "history_load") return seedTranscript;
          if (cmd === "transcribe_file") {
            const input = args.input as string;
            emit("transcribe:progress", {
              path: input,
              phase: "transcribing",
              displayPct: 12,
              elapsedSec: 2,
              etaSec: 10,
            });
            await new Promise((resolve) => setTimeout(resolve, 150));
            if (cancelled.has(input)) throw "cancelled";
            markTranscribed(input);
            return {
              ...seedTranscript,
              duration_ms: rows.find((f) => f.path === input)?.duration_ms ?? 0,
            };
          }
          if (cmd === "cancel_transcribe") {
            cancelled.add(args.input as string);
            return true;
          }
          if (cmd === "suggest_filename") return { topic: "meeting_notes", stamp: "20260506" };
          if (cmd === "rename_file") return "C:\\audio\\meeting_notes_20260506.wav";
          if (cmd === "delete_file") return null;
          if (cmd === "export_transcript") return args.dest;
          if (cmd === "add_to_workdir") {
            rows.push({
              name: "added.wav",
              path: "C:\\audio\\added.wav",
              is_dir: false,
              is_audio: true,
              size_bytes: 1_000,
              modified_ms: 4,
              cache_key: null,
              utterances: null,
              duration_ms: 1_000,
            });
            return "C:\\audio\\added.wav";
          }
          if (cmd === "log_path") return "C:\\logs\\wt.log";
          if (cmd === "log_tail") return "transcribe ok\nnemo-sortformer\n";
          if (cmd === "log_clear") return null;
          if (cmd === "plugin:event|listen") {
            const event = args.event as string;
            const handler = args.handler as number;
            listeners[event] = [...(listeners[event] ?? []), handler];
            return nextEvent++;
          }
          if (cmd === "plugin:event|unlisten") return null;
          if (cmd === "plugin:dialog|open") return null;
          if (cmd === "plugin:dialog|save") return "C:\\exports\\board.txt";
          if (cmd === "plugin:dialog|message") return "Ok";
          throw new Error(`unhandled invoke ${cmd}`);
        },
      };
      window.__WT_TEST__ = { commandLog, emit };
    },
    { seedFiles: files, seedTranscript: transcript },
  );
}

test.beforeEach(async ({ page }) => {
  await installTauriMocks(page);
  await page.goto("/");
  await expect(page.getByRole("button", { name: "Transcribe", exact: true })).toHaveClass(
    /border-primary/,
  );
});

test("loads the transcribe workspace with GPU defaults", async ({ page }) => {
  await expect(page.getByText("board_meeting.wav")).toBeVisible();
  await expect(page.getByText("interview.mp3")).toBeVisible();
  await expect(page.getByText("transcribed")).toBeVisible();
  await expect(page.locator("select").filter({ hasText: "CUDA" })).toHaveValue("cuda");
  await expect(page.getByText("Diarize speakers")).toBeVisible();
  await expect(page.getByRole("button", { name: "Transcribe all" })).toBeEnabled();
});

test("runs the folder queue and updates rows", async ({ page }) => {
  await page.getByRole("button", { name: "Transcribe all" }).click();
  await expect(page.getByText(/queue 1\/2|queue 2\/2/)).toBeVisible();
  await expect(page.getByText("transcribed")).toHaveCount(3);
  await expect(page.getByRole("button", { name: "Transcribe all" })).toBeDisabled();
});

test("stops an in-flight transcription", async ({ page }) => {
  const row = page.getByRole("row").filter({ hasText: "interview.mp3" });
  await row.getByTitle("Transcribe").click();
  await expect(row.getByTitle("Stop transcription")).toBeVisible();
  await row.getByTitle("Stop transcription").click();
  await expect
    .poll(async () => page.evaluate(() => window.__WT_TEST__.commandLog))
    .toContain("cancel_transcribe");
});

test("previews cached transcript with audio and copy action", async ({ page }) => {
  const row = page.getByRole("row").filter({ hasText: "board_meeting.wav" });
  await row.getByTitle("Preview transcript").click();
  await expect(page.getByText("Transcript preview")).toBeVisible();
  await expect(page.locator("audio")).toHaveAttribute("src", /asset:\/\/localhost/);
  await expect(page.getByText("Opening remarks.")).toBeVisible();
  await page.getByRole("button", { name: "Copy" }).click();
});

test("navigates settings, models, and logs", async ({ page }) => {
  await page.getByRole("button", { name: "Models" }).click();
  await expect(page.getByText("sherpa-whisper-turbo")).toBeVisible();
  await expect(page.getByText("nemo-sortformer-v2")).toBeVisible();
  await expect(page.getByText("sherpa-pyannote-titanet")).toBeVisible();
  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByText("Manage transcription parameters")).toBeVisible();
  await expect(page.locator("select").filter({ hasText: "CUDA" })).toHaveValue("cuda");
  await page.getByRole("button", { name: "Logs" }).click();
  await expect(page.getByText("nemo-sortformer")).toBeVisible();
});
