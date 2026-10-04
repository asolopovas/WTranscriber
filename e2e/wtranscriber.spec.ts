import { expect, test, type Page } from "@playwright/test";
import { installTauriMocks } from "./fixtures/tauri";
import { installSyntheticRecording } from "./fixtures/media";

const commands = (page: Page) => page.evaluate(() => window.__WT_TEST__.commandLog);
const commandCount = (page: Page, cmd: string) =>
  page.evaluate(
    (name) => window.__WT_TEST__.commandLog.filter((entry) => entry === name).length,
    cmd,
  );
const selectByLabel = (page: Page, label: string) =>
  page.getByRole("combobox", { name: new RegExp(`^${label}`) });
const rowNamed = (page: Page, name: string) => page.getByRole("listitem").filter({ hasText: name });

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

  await expect(model).toHaveValue("whisper-cpp-large-v3-turbo-q8");
  await expect(model.getByRole("option", { name: "Whisper large-v3-turbo" })).toHaveCount(1);
  await expect(model.getByRole("option", { name: "Parakeet multilingual" })).toHaveCount(1);
});

test("runs the folder queue and updates rows", async ({ page }) => {
  await page.getByRole("button", { name: /Transcribe all/ }).click();
  await expect.poll(() => commandCount(page, "transcribe_file"), { timeout: 5_000 }).toBe(1);
  await expect(rowNamed(page, "field_notes").getByTitle("Stop", { exact: true })).toBeVisible();
  await finishTranscriptions(page);
  await expect.poll(() => commandCount(page, "transcribe_file"), { timeout: 5_000 }).toBe(2);
  await finishTranscriptions(page);
  await expect(rowNamed(page, "interview").getByTitle("Transcript ready — view")).toBeVisible();
});

test("stops an in-flight transcription", async ({ page }) => {
  const row = rowNamed(page, "interview");
  await row.locator('button[title="Transcribe"]').click();
  await expect(row.getByRole("button", { name: /Stop/ })).toBeVisible();
  await row.getByRole("button", { name: /Stop/ }).click();
  await expect.poll(() => commands(page)).toContain("cancel_transcribe");
});

test("appends remaining files without interrupting the active transcription", async ({ page }) => {
  const first = rowNamed(page, "interview");
  const second = rowNamed(page, "field_notes");
  await first.getByTitle("Transcribe", { exact: true }).click();
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(1);
  await page.getByRole("button", { name: /Transcribe all/ }).click();
  await expect(first.getByTitle("Stop", { exact: true })).toBeVisible();
  await expect(second.getByTitle("Stop", { exact: true })).toBeVisible();
  expect(await commandCount(page, "transcribe_file")).toBe(1);
  expect(await commandCount(page, "cancel_all_transcribes")).toBe(0);
  expect(await commandCount(page, "cancel_transcribe")).toBe(0);
  await finishTranscriptions(page);
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(2);
  await expect(first.getByTitle("Transcript ready — view")).toBeVisible();
  await finishTranscriptions(page);
  await expect(second.getByTitle("Transcript ready — view")).toBeVisible();
});

test("queued jobs keep the settings captured when they were added", async ({ page }) => {
  await rowNamed(page, "interview").getByTitle("Transcribe", { exact: true }).click();
  await page.getByRole("button", { name: /Transcribe all/ }).click();
  await selectByLabel(page, "Speakers").selectOption("2");
  await finishTranscriptions(page);
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(2);
  const captured = await page.evaluate(() => window.__WT_TEST__.transcriptionConfigs);
  expect(captured).toHaveLength(2);
  expect(captured[1]).toMatchObject({ speakers: null });
  await finishTranscriptions(page);
  await expect(rowNamed(page, "field_notes").getByTitle("Transcript ready — view")).toBeVisible();
});

test("cancelling a queued file preserves the active file", async ({ page }) => {
  const first = rowNamed(page, "interview");
  const second = rowNamed(page, "field_notes");
  await first.getByTitle("Transcribe", { exact: true }).click();
  await page.getByRole("button", { name: /Transcribe all/ }).click();
  await second.getByTitle("Stop", { exact: true }).click();
  await expect(second.getByTitle("Transcribe", { exact: true })).toBeVisible();
  await expect(first.getByTitle("Stop", { exact: true })).toBeVisible();
  expect(await commandCount(page, "cancel_transcribe")).toBe(0);
  await finishTranscriptions(page);
  await expect(first.getByTitle("Transcript ready — view")).toBeVisible();
  expect(await commandCount(page, "transcribe_file")).toBe(1);
});

test("cancelling an active file continues unrelated queued work", async ({ page }) => {
  const first = rowNamed(page, "interview");
  const second = rowNamed(page, "field_notes");
  await first.getByTitle("Transcribe", { exact: true }).click();
  await page.getByRole("button", { name: /Transcribe all/ }).click();
  await first.getByTitle("Stop", { exact: true }).click();
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(2);
  expect(await commandCount(page, "cancel_all_transcribes")).toBe(0);
  await expect(second.getByTitle("Stop", { exact: true })).toBeVisible();
  await finishTranscriptions(page);
  await expect(second.getByTitle("Transcript ready — view")).toBeVisible();
});

test("a failed active file does not stop the remaining queue", async ({ page }) => {
  const first = rowNamed(page, "interview");
  const second = rowNamed(page, "field_notes");
  await first.getByTitle("Transcribe", { exact: true }).click();
  await page.getByRole("button", { name: /Transcribe all/ }).click();
  await page.evaluate(() => {
    window.__WT_TEST__.failTranscription(window.__WT_TEST__.started[0]);
  });
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(2);
  await finishTranscriptions(page);
  await expect(second.getByTitle("Transcript ready — view")).toBeVisible();
  await expect(first.getByTitle("Transcript ready — view")).toHaveCount(0);
});

test("previews cached transcript details", async ({ page }) => {
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
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
  await page.getByRole("button", { name: "Clear cache", exact: true }).click();
  await expect.poll(() => commands(page)).toContain("reset_app_data");
  await expect(page.getByText("App data cleared", { exact: false })).toBeVisible();
});

test("retries failed files and ignores duplicate starts while they are active", async ({
  page,
}) => {
  const row = rowNamed(page, "interview");
  await row.getByTitle("Transcribe", { exact: true }).click();
  await row.dblclick();
  expect(await commandCount(page, "transcribe_file")).toBe(1);
  await page.evaluate(() => window.__WT_TEST__.failTranscription(window.__WT_TEST__.started[0]));
  await expect(page.getByText("test transcription failed", { exact: true })).toBeVisible();
  await row.getByTitle("Transcribe", { exact: true }).click();
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(2);
  await finishTranscriptions(page);
  await expect(row.getByTitle("Transcript ready — view")).toBeVisible();
  await expect(page.getByText("test transcription failed", { exact: true })).toHaveCount(0);
});

test("retranscribes a completed recording without reusing its cached transcript", async ({
  page,
}, testInfo) => {
  const row = rowNamed(page, "board_meeting");
  await expect(row.getByTitle("Retranscribe", { exact: true })).toBeVisible();
  await row.getByTitle("More", { exact: true }).click();
  await page.screenshot({ path: testInfo.outputPath("retranscribe.png") });
  await page
    .getByRole("button", { name: "Retranscribe", exact: true })
    .filter({ has: page.getByText("refresh", { exact: true }) })
    .click();
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(1);
  const call = await page.evaluate(() =>
    window.__WT_TEST__.commandCalls.find((call) => call.command === "transcribe_file"),
  );
  expect(call?.args).toMatchObject({ force: true });
  expect(await commandCount(page, "redo_diarization")).toBe(0);
  await finishTranscriptions(page);
  await expect(page.getByRole("heading", { name: "Transcript", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Close transcript", exact: true }).click();
  await row.getByTitle("Retranscribe", { exact: true }).click();
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(2);
});

test("offers local quality alignment without replacing the native ASR model", async ({
  page,
}, testInfo) => {
  await rowNamed(page, "board_meeting").getByTitle("More", { exact: true }).click();
  await page.getByRole("button", { name: /Re-diarize…/ }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("combobox", { name: "Diarizer" }).selectOption("whisperx-community1");
  await expect(dialog.getByText(/Realigns words to the audio/)).toBeVisible();
  await expect(dialog.locator("..")).toHaveCSS("opacity", "1");
  await page.screenshot({ path: testInfo.outputPath("quality-desktop.png") });
  await dialog.getByRole("button", { name: "Re-diarize", exact: true }).click();
  await expect.poll(() => commandCount(page, "redo_diarization")).toBe(1);
  const call = await page.evaluate(() =>
    window.__WT_TEST__.commandCalls.find((call) => call.command === "redo_diarization"),
  );
  expect(call?.args).toMatchObject({ config: { diarizer: "whisperx-community1", diarize: true } });
});

test("queues re-diarization behind transcription with its chosen speakers", async ({ page }) => {
  await rowNamed(page, "interview").getByTitle("Transcribe", { exact: true }).click();
  await rowNamed(page, "board_meeting").getByTitle("More", { exact: true }).click();
  await page.getByRole("button", { name: /Re-diarize…/ }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("combobox", { name: "Speakers" }).selectOption("3");
  await dialog.getByRole("button", { name: "Re-diarize", exact: true }).click();
  await expect(rowNamed(page, "board_meeting").getByTitle("Stop", { exact: true })).toBeVisible();
  expect(await commandCount(page, "redo_diarization")).toBe(0);
  await finishTranscriptions(page);
  await expect.poll(() => commandCount(page, "redo_diarization")).toBe(1);
  const call = await page.evaluate(() =>
    window.__WT_TEST__.commandCalls.find((call) => call.command === "redo_diarization"),
  );
  expect(call?.args).toMatchObject({
    oldCacheKey: "board",
    config: { speakers: 3, diarize: true },
  });
  await finishTranscriptions(page);
  await expect(page.getByRole("heading", { name: "Transcript", exact: true })).toBeVisible();
  await expect(page.getByText("Opening remarks.", { exact: true })).toBeVisible();
});

test("renames an audio file and preserves its cached transcript", async ({ page }) => {
  await rowNamed(page, "board_meeting").getByTitle("More", { exact: true }).click();
  await page
    .getByRole("button", { name: /Rename/, exact: false })
    .last()
    .click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("textbox").fill("renamed_meeting.wav");
  await dialog.getByRole("button", { name: "Rename", exact: true }).click();
  await expect(rowNamed(page, "board_meeting")).toHaveCount(0);
  await rowNamed(page, "renamed_meeting").getByTitle("Transcript ready — view").click();
  await expect(page.getByText("Opening remarks.", { exact: true })).toBeVisible();
});

test("exports the chosen transcript format through the native save boundary", async ({ page }) => {
  await rowNamed(page, "board_meeting").getByTitle("More", { exact: true }).click();
  await page.getByRole("button", { name: /Export…/ }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("radio", { name: "Subtitles (.srt)" }).check();
  await dialog.getByRole("button", { name: "Save…", exact: true }).click();
  await expect.poll(() => commands(page)).toContain("plugin:fs|write_text_file");
  const calls = await page.evaluate(() => window.__WT_TEST__.commandCalls);
  expect(calls.find((call) => call.command === "format_transcript")?.args).toMatchObject({
    format: "srt",
    transcript: { language: "en" },
  });
  expect(calls.find((call) => call.command === "plugin:dialog|save")?.args).toMatchObject({
    options: { defaultPath: "board_meeting.srt" },
  });
});

test("imports audio from the file picker and ignores repeated or unsupported dropped paths", async ({
  page,
}) => {
  await page.evaluate(() => window.__WT_TEST__.setOpenPaths(["C:\\incoming\\new_recording.wav"]));
  await page.getByRole("button", { name: "Add audio files", exact: true }).click();
  await expect(rowNamed(page, "new_recording")).toBeVisible();
  await expect.poll(() => commandCount(page, "add_to_workdir")).toBe(1);
  await page.evaluate(() => {
    window.__WT_TEST__.emit("tauri://drag-drop", {
      paths: ["C:\\incoming\\dropped.wav", "C:\\incoming\\dropped.wav", "C:\\incoming\\notes.txt"],
      position: { x: 10, y: 10 },
    });
  });
  await expect(rowNamed(page, "dropped")).toHaveCount(1);
  await expect.poll(() => commandCount(page, "add_to_workdir")).toBe(2);
  await expect(rowNamed(page, "notes.txt")).toHaveCount(0);
});

test("transcribes selected files without including the deselected cached file", async ({
  page,
}) => {
  await page.keyboard.press("Control+a");
  await expect(page.getByText("3 selected", { exact: true })).toBeVisible();
  await rowNamed(page, "board_meeting").getByRole("checkbox").uncheck();
  await page.getByRole("button", { name: "Transcribe", exact: true }).first().click();
  await page.getByRole("button", { name: "Clear", exact: true }).click();
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(1);
  await finishTranscriptions(page);
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(2);
  await finishTranscriptions(page);
  await expect(rowNamed(page, "field_notes").getByTitle("Transcript ready — view")).toBeVisible();
  const paths = await page.evaluate(() => window.__WT_TEST__.started);
  expect(paths).toEqual(["C:\\audio\\interview.mp3", "C:\\audio\\field_notes.m4a"]);
});

test("opens logs from a job error and clears the displayed log", async ({ page }) => {
  await rowNamed(page, "interview").getByTitle("Transcribe", { exact: true }).click();
  await page.evaluate(() => window.__WT_TEST__.failTranscription(window.__WT_TEST__.started[0]));
  await page.getByRole("button", { name: "View log", exact: true }).click();
  await expect(page.getByText("transcribe ok", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Clear log", exact: true }).click();
  await expect(page.getByText("(log is empty)", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Transcribe", exact: true }).click();
  await expect(rowNamed(page, "interview")).toBeVisible();
});

test("editing text and speaker names cannot select or delete recordings", async ({ page }) => {
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  await page.getByRole("button", { name: "Edit segment 1", exact: true }).click();
  const text = page.getByRole("textbox", { name: "Segment text" });
  await text.press("Control+a");
  await text.press("Backspace");
  await expect(text).toHaveValue("");
  expect(await commandCount(page, "delete_file")).toBe(0);
  await expect(page.getByText("3 selected", { exact: true })).toHaveCount(0);
  await page.getByRole("button", { name: "Cancel", exact: true }).click();
  await page.getByTitle("Rename speaker").first().click();
  const name = page.getByRole("combobox", { name: "Speaker name", exact: true });
  await name.press("Control+a");
  await name.press("Backspace");
  expect(await commandCount(page, "delete_file")).toBe(0);
  await expect(page.getByRole("listitem")).toHaveCount(3);
});

test("bulk removal requires confirmation and Backspace cannot remove files", async ({ page }) => {
  await rowNamed(page, "board_meeting").click();
  await page.keyboard.press("Control+a");
  await expect(page.getByText("3 selected", { exact: true })).toBeVisible();
  await page.keyboard.press("Backspace");
  expect(await commandCount(page, "delete_file")).toBe(0);
  await page.evaluate(() => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (cmd, args, options) => {
      if (cmd === "plugin:dialog|message") {
        window.__WT_TEST__.commandCalls.push({ command: cmd, args: args ?? {} });
        return "Cancel";
      }
      if (cmd === "plugin:dialog|confirm") return false;
      return invoke(cmd, args, options);
    };
  });
  await page.keyboard.press("Delete");
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.__WT_TEST__.commandCalls.some(
          (call) =>
            call.command === "plugin:dialog|message" &&
            String(call.args.message).includes("3 recordings"),
        ),
      ),
    )
    .toBe(true);
  expect(await commandCount(page, "delete_file")).toBe(0);
  await expect(page.getByRole("listitem")).toHaveCount(3);
});

test("deletes only the file targeted by its menu", async ({ page }) => {
  await rowNamed(page, "board_meeting").click();
  await rowNamed(page, "field_notes").getByTitle("More", { exact: true }).click();
  await page.getByRole("button", { name: "Delete", exact: true }).click();
  await expect(rowNamed(page, "field_notes")).toHaveCount(0);
  await expect(rowNamed(page, "board_meeting")).toBeVisible();
  const removed = await page.evaluate(() =>
    window.__WT_TEST__.commandCalls.find((call) => call.command === "delete_file"),
  );
  expect(removed?.args).toEqual({ path: "C:\\audio\\field_notes.m4a" });
});

test("preloads waveforms and opens independent trims from desktop row shortcuts", async ({
  page,
}, testInfo) => {
  await expect.poll(() => commandCount(page, "audio_waveform")).toBe(3);
  expect(await commandCount(page, "read_audio_bytes")).toBe(0);
  const first = rowNamed(page, "interview");
  const second = rowNamed(page, "field_notes");
  const trim = first.getByTitle("Trim recording", { exact: true });
  const ai = first.getByTitle("Auto-rename (AI)", { exact: true });
  await expect(trim).toBeVisible();
  const aiBox = await ai.boundingBox();
  const trimBox = await trim.boundingBox();
  expect(trimBox!.x).toBeGreaterThan(aiBox!.x);
  expect(Math.abs(trimBox!.y - aiBox!.y)).toBeLessThan(2);
  await page.screenshot({ path: testInfo.outputPath("desktop-trim-shortcut.png") });
  await trim.click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("selected 01:00", { exact: true })).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("i");
  await dialog.getByTitle("Save", { exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await second.getByTitle("Trim recording", { exact: true }).click();
  await expect(dialog.getByText("selected 01:00", { exact: true })).toBeVisible();
  await expect(dialog.getByText("Position 00:00", { exact: true })).toBeVisible();
  await dialog.getByTitle("Close", { exact: true }).click();
  await trim.click();
  await expect(dialog.getByText("selected 00:55", { exact: true })).toBeVisible();
  await expect(dialog.getByText("Position 00:05", { exact: true })).toBeVisible();
  await dialog.getByTitle("Close", { exact: true }).click();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(trim).toBeHidden();
  await first.getByTitle("More", { exact: true }).click();
  await expect(page.getByRole("button", { name: /Cut: 00:05/ })).toBeVisible();
  await page.screenshot({ path: testInfo.outputPath("mobile-trim-menu.png") });
});

test("saves a trim range and resets it to the full track", async ({ page }) => {
  const row = rowNamed(page, "field_notes");
  await row.getByTitle("More", { exact: true }).click();
  await page.getByRole("button", { name: "Cut / select range", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("selected 01:00", { exact: true })).toBeVisible();
  const box = await dialog.locator("canvas").boundingBox();
  expect(box).not.toBeNull();
  await page.mouse.click(box!.x + box!.width * 0.25, box!.y + box!.height / 2);
  await dialog.getByTitle("Mark in (I) — set start to playhead", { exact: true }).click();
  await page.mouse.click(box!.x + box!.width * 0.75, box!.y + box!.height / 2);
  await dialog.getByTitle("Mark out (O) — set end to playhead", { exact: true }).click();
  await expect(dialog.getByText("selected 00:30", { exact: true })).toBeVisible();
  await dialog.getByTitle("Save", { exact: true }).click();
  await expect(row.getByText("trimmed", { exact: true })).toBeVisible();
  const saved = await page.evaluate(() =>
    window.__WT_TEST__.commandCalls.find((call) => call.command === "save_audio_meta"),
  );
  expect(saved?.args.meta).toMatchObject({ trim_start_ms: 15000, trim_end_ms: 45000 });
  await row.getByTitle("More", { exact: true }).click();
  await page.getByRole("button", { name: /Cut: 00:15/ }).click();
  await expect(dialog.getByText("selected 00:30", { exact: true })).toBeVisible();
  await dialog.getByTitle("Reset to full track", { exact: true }).click();
  await expect(dialog.getByText("selected 01:00", { exact: true })).toBeVisible();
  await dialog.getByTitle("Save", { exact: true }).click();
  await expect(row.getByText("trimmed", { exact: true })).toHaveCount(0);
});

test("seeks in the trim editor with a saved step and leaves arrow keys in fields", async ({
  page,
}, testInfo) => {
  const openTrim = async () => {
    await rowNamed(page, "field_notes").getByTitle("More", { exact: true }).click();
    await page.getByRole("button", { name: "Cut / select range", exact: true }).click();
  };
  await openTrim();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("selected 01:00", { exact: true })).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await expect(dialog.getByText("Position 00:05", { exact: true })).toBeVisible();
  const step = dialog.getByRole("spinbutton", { name: "Arrow-key step (seconds)" });
  await step.fill("12");
  await step.press("ArrowLeft");
  await expect(dialog.getByText("Position 00:05", { exact: true })).toBeVisible();
  await step.press("Tab");
  await page.keyboard.press("ArrowRight");
  await expect(dialog.getByText("Position 00:17", { exact: true })).toBeVisible();
  for (let i = 0; i < 5; i += 1) await page.keyboard.press("ArrowRight");
  await expect(dialog.getByText("Position 01:00", { exact: true })).toBeVisible();
  for (let i = 0; i < 6; i += 1) await page.keyboard.press("ArrowLeft");
  await expect(dialog.getByText("Position 00:00", { exact: true })).toBeVisible();
  await dialog.getByTitle("Close", { exact: true }).click();
  await openTrim();
  await expect(step).toHaveValue("12");
  await expect(dialog.locator("..")).toHaveCSS("opacity", "1");
  await page.screenshot({ path: testInfo.outputPath("trim-desktop.png") });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(step).toBeVisible();
  await expect(dialog.getByTitle("Save", { exact: true })).toBeInViewport();
  await page.screenshot({ path: testInfo.outputPath("trim-mobile.png") });
});

test("renames a transcript speaker and copies the transcript", async ({ page }) => {
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  await page.getByRole("button", { name: "SPEAKER_01", exact: true }).click();
  const modal = page.getByRole("dialog", { name: "Rename speaker" });
  await expect(modal).toBeVisible();
  const editor = modal.getByRole("combobox", { name: "Speaker name" });
  await expect(editor).toBeFocused();
  await editor.fill("Alice");
  await editor.press("Enter");
  await expect(page.getByRole("button", { name: "Alice", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Copy transcript", exact: true }).click();
  await expect(page.getByRole("button", { name: "Copied", exact: true })).toBeVisible();
  expect(await page.evaluate(() => window.__WT_TEST__.clipboardText)).toContain("Opening remarks.");
  await page.getByRole("button", { name: "Close transcript", exact: true }).click();
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  await expect(page.getByRole("button", { name: "Alice", exact: true })).toBeVisible();
});

test("changes models with the matching engine and blocks a missing model until downloaded", async ({
  page,
}) => {
  await selectByLabel(page, "Model").selectOption("parakeet-tdt-0.6b-v3-int8");
  await expect
    .poll(() => page.evaluate(() => window.__WT_TEST__.savedConfigs.at(-1)))
    .toMatchObject({ model: "parakeet-tdt-0.6b-v3-int8", engine: "parakeet" });
  await page.evaluate(() =>
    window.__WT_TEST__.setModelInstalled("parakeet-tdt-0.6b-v3-int8", false),
  );
  await expect(page.getByText("Model not installed", { exact: true })).toBeVisible();
  await rowNamed(page, "interview").getByTitle("Transcribe", { exact: true }).click();
  await expect(page.getByText(/Download it in Configuration\./)).toBeVisible();
  expect(await commandCount(page, "transcribe_file")).toBe(0);
  await page.getByRole("button", { name: "Download", exact: true }).click();
  await expect(page.getByText("Model not installed", { exact: true })).toHaveCount(0);
  await rowNamed(page, "interview").getByTitle("Transcribe", { exact: true }).click();
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(1);
  await finishTranscriptions(page);
  await expect(rowNamed(page, "interview").getByTitle("Transcript ready — view")).toBeVisible();
});

test("records synthetic input, saves mono 16 kHz WAV bytes, and releases its media tracks", async ({
  page,
}) => {
  await installSyntheticRecording(page);
  await page.reload();
  await expect(rowNamed(page, "interview")).toBeVisible();
  await page.getByTitle("Record", { exact: true }).click();
  await expect(page.getByTitle(/^Stop recording/)).toBeVisible();
  await page.getByTitle(/^Stop recording/).click();
  await expect.poll(() => commands(page)).toContain("save_recording");
  await expect(rowNamed(page, "recording")).toBeVisible();
  const result = await page.evaluate(() => ({
    bytes: window.__WT_TEST__.recordingBytes,
    media: window.__WT_MEDIA__,
  }));
  expect(result.media).toEqual({ requestedAudio: true, stoppedTracks: 1 });
  const bytes = Uint8Array.from(result.bytes);
  const view = new DataView(bytes.buffer);
  expect(new TextDecoder().decode(bytes.slice(0, 4))).toBe("RIFF");
  expect(view.getUint16(22, true)).toBe(1);
  expect(view.getUint32(24, true)).toBe(16000);
  expect(bytes.length).toBe(364);
  expect(view.getInt16(44, true)).toBe(4095);
});

test("shows optional timing review and plays the original filtered segment", async ({
  page,
}, testInfo) => {
  await page.evaluate(() => {
    const invoke = window.__TAURI_INTERNALS__.invoke;
    window.__TAURI_INTERNALS__.invoke = async (cmd, args, options) => {
      const result = await invoke(cmd, args, options);
      if (cmd !== "history_load") return result;
      return {
        ...(result as object),
        diarizer: "whisperx-community-1-v1",
        words: [
          { text: "Opening", start_ms: 0, end_ms: 500, speaker: "SPEAKER_01", confidence: 0.9 },
          { text: "answer.", start_ms: 3000, end_ms: 4000, speaker: "SPEAKER_02", confidence: 0.2 },
        ],
      };
    };
  });
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  await page.getByText("Timing review: 1 of 2 words", { exact: true }).click();
  await page.getByRole("button", { name: "Show 1 segment to review", exact: true }).click();
  await expect(page.getByText("Lower-scoring words: answer.", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Play segment 1", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Play segment 2", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Play segment 2", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.__WT_TEST__.commandCalls
          .filter((call) => call.command === "read_audio_segment")
          .some((call) => call.args.startMs === 2000 && call.args.endMs === 4000),
      ),
    )
    .toBe(true);
  await page.screenshot({ path: testInfo.outputPath("timing-desktop.png") });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: testInfo.outputPath("timing-mobile.png") });
  await page.getByRole("button", { name: "Show all segments", exact: true }).click();
  await expect(page.getByRole("button", { name: "Play segment 1", exact: true })).toBeVisible();
});

test("edits a completed segment and plays only its audio range", async ({ page }, testInfo) => {
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  await page.getByRole("button", { name: "Edit segment 1", exact: true }).click();
  await page.getByRole("textbox", { name: "Segment text" }).fill("Corrected opening remarks.");
  await page.getByRole("button", { name: "Save text", exact: true }).click();
  await expect(page.getByText("Corrected opening remarks.", { exact: true })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Saved to .txt" })).toBeVisible();
  await page.getByRole("button", { name: "Play segment 2", exact: true }).click();
  await expect.poll(() => commandCount(page, "read_audio_segment")).toBeGreaterThanOrEqual(2);
  await expect(page.getByRole("button", { name: "Stop segment 2", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Play segment 2", exact: true })).toBeVisible();
  const stopped = await page.locator("audio").evaluate((element: HTMLAudioElement) => ({
    paused: element.paused,
    time: element.currentTime,
  }));
  expect(stopped.paused).toBe(true);
  expect(stopped.time).toBeLessThanOrEqual(2.01);
  expect(await commandCount(page, "read_audio_bytes")).toBe(0);
  await page.screenshot({ path: testInfo.outputPath("transcript-desktop.png") });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: testInfo.outputPath("transcript-mobile.png") });
  await page.getByRole("button", { name: "Close transcript", exact: true }).click();
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  await expect(page.getByText("Corrected opening remarks.", { exact: true })).toBeVisible();
});

test("keeps the saved trim after processing finishes", async ({ page }) => {
  const row = rowNamed(page, "field_notes");
  await row.getByTitle("More", { exact: true }).click();
  await page.getByRole("button", { name: "Cut / select range", exact: true }).click();
  const dialog = page.getByRole("dialog");
  await expect(dialog.getByText("selected 01:00", { exact: true })).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("i");
  await dialog.getByTitle("Save", { exact: true }).click();
  await row.getByTitle("Transcribe", { exact: true }).click();
  await expect.poll(() => commandCount(page, "transcribe_file")).toBe(1);
  await finishTranscriptions(page);
  await expect(row.getByTitle("Transcript ready — view")).toBeVisible();
  await expect(row.getByText("trimmed", { exact: true })).toBeVisible();
  await row.getByTitle("More", { exact: true }).click();
  await page.getByRole("button", { name: /Cut: 00:05/ }).click();
  await expect(dialog.getByText("selected 00:55", { exact: true })).toBeVisible();
  expect(await commandCount(page, "apply_trim")).toBe(0);
});

test("plays exact ranges even when the old padding preference is enabled", async ({ page }) => {
  await page.evaluate(() => localStorage.setItem("wt.transcriptPlaybackContext", "true"));
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  await expect(page.getByRole("switch", { name: "Include context" })).toHaveCount(0);
  await page.getByRole("button", { name: "Play segment 2", exact: true }).click();
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.__WT_TEST__.commandCalls.some(
          (call) =>
            call.command === "read_audio_segment" &&
            call.args.startMs === 2000 &&
            call.args.endMs === 4000,
        ),
      ),
    )
    .toBe(true);
});

test("corrects one speaker and replaces text across the current transcript", async ({
  page,
}, testInfo) => {
  const openTranscript = () =>
    rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  await openTranscript();
  await page.getByRole("button", { name: "SPEAKER_01", exact: true }).click();
  const speakerDialog = page.getByRole("dialog", { name: "Rename speaker" });
  await speakerDialog.getByRole("combobox", { name: "Apply to" }).selectOption("segment");
  await speakerDialog.getByRole("combobox", { name: "Speaker name" }).fill("Alice");
  await expect(speakerDialog.locator("..")).toHaveCSS("opacity", "1");
  await page.screenshot({ path: testInfo.outputPath("single-speaker-desktop.png") });
  await speakerDialog.getByRole("button", { name: "Rename", exact: true }).click();
  await expect(page.getByRole("button", { name: "Alice", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "SPEAKER_02", exact: true })).toBeVisible();
  expect(await commandCount(page, "rename_speaker")).toBe(0);
  await page.getByRole("button", { name: "Find and replace", exact: true }).click();
  const replaceDialog = page.getByRole("dialog", { name: "Find and replace" });
  await replaceDialog.getByRole("textbox", { name: "Find", exact: true }).fill(".");
  await replaceDialog.getByRole("textbox", { name: "Replace with", exact: true }).fill("!");
  await expect(replaceDialog.getByText("2 matches in 2 segments", { exact: true })).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(
    replaceDialog.getByRole("button", { name: "Replace all", exact: true }),
  ).toBeInViewport();
  await expect(replaceDialog.locator("..")).toHaveCSS("opacity", "1");
  await page.screenshot({ path: testInfo.outputPath("replace-mobile.png") });
  await replaceDialog.getByRole("button", { name: "Replace all", exact: true }).click();
  await expect(replaceDialog).not.toBeVisible();
  await expect(page.getByText("Opening remarks!", { exact: true })).toBeVisible();
  await expect(page.getByText("Follow up answer!", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Close transcript", exact: true }).click();
  await page.setViewportSize({ width: 1280, height: 720 });
  await openTranscript();
  await expect(page.getByRole("button", { name: "Alice", exact: true })).toBeVisible();
  await expect(page.getByText("Opening remarks!", { exact: true })).toBeVisible();
});

test("edits in a responsive transcript modal with persistent undo, review and segment retry", async ({
  page,
}, testInfo) => {
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  const modal = page.getByRole("dialog", { name: "Transcript", exact: true });
  await expect(modal).toBeVisible();
  expect((await modal.boundingBox())!.width).toBe(1000);
  await page.getByRole("button", { name: "SPEAKER_01", exact: true }).click();
  const rename = page.getByRole("dialog", { name: "Rename speaker" });
  await expect(rename.getByRole("combobox", { name: "Apply to" })).toHaveValue("segment");
  await expect(rename.getByText(/Change only segment|saved text transcript/)).toHaveCount(0);
  await rename.getByRole("combobox", { name: "Speaker name" }).fill("Alice");
  await rename.getByRole("button", { name: "Rename", exact: true }).click();
  await expect(modal.getByRole("button", { name: "Alice", exact: true })).toBeFocused();
  await modal.getByRole("button", { name: "Undo edit", exact: true }).click();
  await expect(modal.getByRole("button", { name: "SPEAKER_01", exact: true })).toBeVisible();
  await modal.getByRole("button", { name: "Mark for review: segment 2", exact: true }).click();
  await modal.getByRole("button", { name: "Show 1 segment to review", exact: true }).click();
  await expect(modal.locator("article")).toHaveCount(1);
  await modal.getByRole("button", { name: "Retry recognition for segment 2", exact: true }).click();
  await expect(modal.getByText("Recognised segment.", { exact: true })).toBeVisible();
  const retry = await page.evaluate(() =>
    window.__WT_TEST__.commandCalls.find((call) => call.command === "retry_transcript_segment"),
  );
  expect(retry?.args).toMatchObject({ index: 1, key: "board" });
  await modal.getByRole("button", { name: "Show all segments", exact: true }).click();
  await modal.getByRole("button", { name: "Delete segment 1", exact: true }).click();
  await expect(modal.locator("article")).toHaveCount(1);
  await page.keyboard.press("Control+z");
  await expect(modal.locator("article")).toHaveCount(2);
  await expect(modal.getByText("Opening remarks.", { exact: true })).toBeVisible();
  await expect(modal.locator("..")).toHaveCSS("opacity", "1");
  await page.screenshot({ path: testInfo.outputPath("transcript-modal-desktop.png") });
  await modal.getByRole("button", { name: "Close transcript", exact: true }).click();
  await rowNamed(page, "board_meeting").getByTitle("Transcript ready — view").click();
  await expect(modal.getByRole("button", { name: "Undo edit", exact: true })).toBeEnabled();
  await page.setViewportSize({ width: 390, height: 844 });
  expect((await modal.boundingBox())!.width).toBeLessThan(390);
  await expect(
    modal.getByRole("button", { name: "Close transcript", exact: true }),
  ).toBeInViewport();
  await page.screenshot({ path: testInfo.outputPath("transcript-modal-mobile.png") });
});
