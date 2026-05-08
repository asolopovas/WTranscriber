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
      savedConfigs: unknown[];
      emit: (event: string, payload: unknown) => void;
      finishTranscriptions: () => void;
    };
  }
}

const audioDir = "C:\\audio";

const audioFile = (name: string, overrides: Record<string, unknown> = {}) => ({
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

const config = {
  model: "sherpa-whisper-turbo",
  engine: "whisper-onnx",
  language: "en",
  device: "cuda",
  threads: 8,
  diarize: true,
  speakers: null,
  diarizer: "auto",
  auto_rename: false,
  last_dir: audioDir,
  use_persistent_models: true,
};

const models = [
  [
    "sherpa-whisper-turbo",
    "asr",
    "whisper-onnx",
    "Whisper large-v3-turbo (ONNX, multilingual)",
    true,
  ],
  ["sherpa-zipformer-en", "asr", "zipformer", "Zipformer English", false],
  [
    "nemo-sortformer-v2",
    "diarizer",
    "nemo-sortformer",
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

async function installTauriMocks(page: Page) {
  await page.addInitScript(
    ({ seedFiles, seedTranscript, seedConfig, seedModels }) => {
      const callbacks: Record<number, (event: unknown) => void> = {};
      const listeners: Record<string, number[]> = {};
      const commandLog: string[] = [];
      const savedConfigs: unknown[] = [];
      const rows = seedFiles.map((file) => ({ ...file }));
      const cancelled = new Set<string>();
      const pending = new Map<string, () => void>();
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
        load_config: () => seedConfig,
        save_config: (args) => savedConfigs.push(args.config) && null,
        list_models: () => seedModels,
        audio_waveform: () => Array.from({ length: 160 }, (_, i) => 0.15 + ((i * 17) % 80) / 100),
        default_dir: () => "C:\\audio",
        list_directory: () => ({ path: "C:\\audio", parent: null, entries: rows }),
        history_load: () => seedTranscript,
        suggest_filename: () => ({ topic: "meeting_notes", stamp: "20260506" }),
        rename_file: () => "C:\\audio\\meeting_notes_20260506.wav",
        delete_file: () => null,
        export_transcript: (args) => args.dest,
        add_to_workdir: () => {
          rows.push({ ...seedFiles[1], name: "added.wav", path: "C:\\audio\\added.wav" });
          return "C:\\audio\\added.wav";
        },
        log_path: () => "C:\\logs\\wt.log",
        log_tail: () => "transcribe ok\nnemo-sortformer\n",
        log_clear: () => null,
        reset_transcript_cache: () => 3,
        reset_audio_cache: () => 2,
        "plugin:event|listen": (args) => {
          const event = args.event as string;
          listeners[event] = [...(listeners[event] ?? []), args.handler as number];
          return nextEvent++;
        },
        "plugin:event|unlisten": () => null,
        "plugin:dialog|open": () => null,
        "plugin:dialog|save": () => "C:\\exports\\board.txt",
        "plugin:dialog|message": () => "Ok",
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
        async invoke(cmd: string, args: Record<string, unknown> = {}) {
          commandLog.push(cmd);
          if (cmd === "transcribe_file") {
            const input = args.input as string;
            emit("transcribe:progress", {
              path: input,
              phase: "transcribing",
              displayPct: 12,
              elapsedSec: 2,
              etaSec: 10,
            });
            await new Promise<void>((resolve) => pending.set(input, resolve));
            pending.delete(input);
            if (cancelled.has(input)) throw "cancelled";
            markTranscribed(input);
            return {
              ...seedTranscript,
              duration_ms: rows.find((file) => file.path === input)?.duration_ms ?? 0,
            };
          }
          if (cmd === "cancel_transcribe") {
            cancelled.add(args.input as string);
            pending.get(args.input as string)?.();
            return true;
          }
          const reply = replies[cmd];
          if (reply) return reply(args);
          throw new Error(`unhandled invoke ${cmd}`);
        },
      };
      window.__WT_TEST__ = { commandLog, savedConfigs, emit, finishTranscriptions };
    },
    { seedFiles: files, seedTranscript: transcript, seedConfig: config, seedModels: models },
  );
}

const commands = (page: Page) => page.evaluate(() => window.__WT_TEST__.commandLog);
const commandCount = (page: Page, cmd: string) =>
  page.evaluate(
    (name) => window.__WT_TEST__.commandLog.filter((entry) => entry === name).length,
    cmd,
  );
const selectByLabel = (page: Page, label: string) =>
  page.locator("label").filter({ hasText: label }).locator("select");
const rowNamed = (page: Page, name: string) => page.getByRole("row").filter({ hasText: name });

async function finishTranscriptions(page: Page) {
  await page.evaluate(() => window.__WT_TEST__.finishTranscriptions());
}

test.beforeEach(async ({ page }) => {
  await installTauriMocks(page);
  await page.goto("/");
  await expect.poll(() => commands(page)).toContain("list_directory");
});

test("loads persisted GPU transcription configuration", async ({ page }) => {
  await expect(selectByLabel(page, "Device")).toHaveValue("cuda");
  await expect(page.getByRole("button", { name: "Transcribe all" })).toBeEnabled();
  await expect.poll(() => commands(page)).toContain("load_config");
});

test("loads compatible model choices", async ({ page }) => {
  const model = selectByLabel(page, "Model");

  await expect(model).toHaveValue("sherpa-whisper-turbo");
  await expect(model.getByRole("option", { name: "Whisper large-v3-turbo" })).toHaveCount(1);
  await expect(model.getByRole("option", { name: "Zipformer English" })).toHaveCount(1);
});

test("runs the folder queue and updates rows", async ({ page }) => {
  await page.getByRole("button", { name: /Transcribe all/ }).click();
  await expect.poll(() => commandCount(page, "transcribe_file"), { timeout: 5_000 }).toBe(1);
  await expect(page.getByRole("button", { name: /Transcribe all/ })).toBeDisabled();
  await finishTranscriptions(page);
  await expect.poll(() => commandCount(page, "transcribe_file"), { timeout: 5_000 }).toBe(2);
  await finishTranscriptions(page);
  await expect(rowNamed(page, "interview")).toContainText("transcribed");
});

test("stops an in-flight transcription", async ({ page }) => {
  const row = rowNamed(page, "interview");
  await row.locator('button[title="Transcribe"]').click();
  await expect(row.getByRole("button", { name: /Stop/ })).toBeVisible();
  await row.getByRole("button", { name: /Stop/ }).click();
  await expect.poll(() => commands(page)).toContain("cancel_transcribe");
});

test("previews cached transcript details", async ({ page }) => {
  await rowNamed(page, "board_meeting").click();
  await expect.poll(() => commands(page)).toContain("history_load");
  await expect(page.getByRole("heading", { name: "Transcript" })).toBeVisible();
  await expect(page.getByText("Opening remarks.")).toBeVisible();
  await expect(page.getByText("Follow up answer.")).toBeVisible();
});

test("persists transcription options and resets caches", async ({ page }) => {
  await selectByLabel(page, "Speakers").selectOption("2");
  await page.getByRole("switch", { name: "Auto-Rename" }).click();
  await expect
    .poll(async () =>
      page.evaluate(() =>
        window.__WT_TEST__.savedConfigs.some(
          (cfg) =>
            typeof cfg === "object" &&
            cfg !== null &&
            (cfg as { speakers?: unknown; auto_rename?: unknown }).speakers === 2 &&
            (cfg as { speakers?: unknown; auto_rename?: unknown }).auto_rename === true,
        ),
      ),
    )
    .toBe(true);

  await page.getByRole("button", { name: "Settings" }).click();
  for (const name of ["Reset transcript cache", "Reset audio cache"])
    await page.getByRole("button", { name }).click();
  await expect.poll(() => commands(page)).toContain("reset_transcript_cache");
  await expect.poll(() => commands(page)).toContain("reset_audio_cache");
});
