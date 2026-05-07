<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { open, save, confirm, message } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, events } from "./api";
import type {
  AudioMeta,
  Config,
  DirEntry,
  DirListing,
  ExportFormat,
  FileProgress,
  ModelInfo,
  SystemInfo,
  TranscribeProgress,
  Transcript,
} from "./types";
import Settings from "./components/Settings.vue";
import LogViewer from "./components/LogViewer.vue";
import Recorder from "./components/Recorder.vue";
import SetupGate from "./components/SetupGate.vue";
import TranscribeIcon from "./components/icons/TranscribeIcon.vue";
import SaveIcon from "./components/icons/SaveIcon.vue";
import CancelIcon from "./components/icons/CancelIcon.vue";
import Spinner from "./components/icons/Spinner.vue";
import SettingsIcon from "./components/icons/SettingsIcon.vue";
import LogIcon from "./components/icons/LogIcon.vue";

type Tab = "transcribe" | "settings" | "logs";

const tab = ref<Tab>("transcribe");
const version = ref("");
const sys = ref<SystemInfo | null>(null);
const config = ref<Config | null>(null);
const models = ref<ModelInfo[]>([]);
const essentialIds = ref<string[]>([]);
const essentialProgress = ref<Record<string, FileProgress>>({});
const essentialErrors = ref<Record<string, true>>({});
const essentialsForceReady = ref(false);
const essentialsReady = computed(() => {
  if (essentialsForceReady.value) return true;
  if (!essentialIds.value.length) return true;
  return essentialIds.value.every((id) => {
    const m = models.value.find((x) => x.id === id);
    return m?.status === "installed";
  });
});
const listing = ref<DirListing | null>(null);
const selectedPath = ref<string>("");
const transcript = ref<Transcript | null>(null);
const status = ref<"idle" | "running" | "renaming" | "error">("idle");
const error = ref<string | null>(null);
const dragOver = ref(false);
const saveState = ref<"idle" | "saving" | "saved">("idle");
const busy = ref<Record<string, boolean>>({});
const progressByPath = ref<Record<string, TranscribeProgress>>({});
const dialogOpen = ref(false);
const queueActive = ref(false);
const queueTotal = ref(0);
const queueDone = ref(0);
const configOpen = ref(
  typeof window !== "undefined" && window.matchMedia("(min-width: 768px)").matches,
);
const recorderRef = ref<InstanceType<typeof Recorder> | null>(null);
const openMenuPath = ref<string | null>(null);
function toggleMenu(path: string) {
  openMenuPath.value = openMenuPath.value === path ? null : path;
}
function closeMenus() {
  openMenuPath.value = null;
}

function prettyName(name: string): { display: string; timestamp: string | null } {
  const noExt = name.replace(/\.[^.]+$/, "");
  const patterns: { re: RegExp; full4: boolean }[] = [
    { re: /[-_](\d{4})-(\d{2})-(\d{2})[-_T](\d{2})-(\d{2})-(\d{2})$/, full4: true },
    { re: /[-_](\d{4})(\d{2})(\d{2})[-_](\d{2})(\d{2})(\d{2})$/, full4: true },
    { re: /[-_](\d{2})(\d{2})(\d{2})[-_](\d{2})(\d{2})(\d{2})$/, full4: false },
  ];
  for (const { re, full4 } of patterns) {
    const m = noExt.match(re);
    if (m && m.index !== undefined) {
      const display = noExt.slice(0, m.index);
      const [, a, b, c, d, e] = m;
      const yy = full4 ? a.slice(2) : a;
      const months = [
        "Jan",
        "Feb",
        "Mar",
        "Apr",
        "May",
        "Jun",
        "Jul",
        "Aug",
        "Sep",
        "Oct",
        "Nov",
        "Dec",
      ];
      const mi = parseInt(b, 10) - 1;
      const mon = mi >= 0 && mi < 12 ? months[mi] : b;
      return { display: display || noExt, timestamp: `${yy}-${mon}-${c} ${d}:${e}` };
    }
  }
  return { display: noExt, timestamp: null };
}

function fmtClock(secs: number): string {
  if (!Number.isFinite(secs) || secs < 0) secs = 0;
  const total = Math.round(secs);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

function phaseLabel(p: TranscribeProgress["phase"]): string {
  switch (p) {
    case "cache_check":
      return "checking cache";
    case "loading_audio":
      return "loading audio";
    case "transcribing":
      return "transcribing";
    case "diarizing":
      return "diarizing";
    case "writing":
      return "writing";
    case "done":
      return "done";
  }
}

async function withDialog<T>(fn: () => Promise<T>): Promise<T | undefined> {
  if (dialogOpen.value) return undefined;
  dialogOpen.value = true;
  try {
    return await fn();
  } finally {
    dialogOpen.value = false;
  }
}

const tabs: { id: Tab; label: string }[] = [
  { id: "transcribe", label: "Transcribe" },
  { id: "settings", label: "Settings" },
  { id: "logs", label: "Logs" },
];

const allLanguageOptions = [
  "auto",
  "en",
  "de",
  "fr",
  "es",
  "it",
  "pt",
  "nl",
  "pl",
  "ru",
  "uk",
  "zh",
  "ja",
  "ko",
  "ar",
  "tr",
  "hi",
];

const exportFormats: { value: ExportFormat; label: string }[] = [
  { value: "txt", label: "Plain text (.txt)" },
  { value: "csv", label: "CSV (.csv)" },
  { value: "json", label: "JSON (.json)" },
  { value: "srt", label: "Subtitles (.srt)" },
  { value: "vtt", label: "WebVTT (.vtt)" },
];

const asrModels = computed(() =>
  models.value.filter((m) => m.family === "asr" && m.status === "installed"),
);

const allAsrModels = computed(() => models.value.filter((m) => m.family === "asr"));

const selectedAsrModel = computed(
  () => allAsrModels.value.find((m) => m.id === config.value?.model) ?? null,
);

const selectedModelInstalled = computed(() => selectedAsrModel.value?.status === "installed");

const installingSelected = ref(false);
async function installSelectedModel() {
  const id = config.value?.model;
  if (!id) return;
  installingSelected.value = true;
  try {
    await api.installModel(id);
    models.value = await api.listModels();
  } catch (e) {
    console.error("install failed", e);
  } finally {
    installingSelected.value = false;
  }
}

const speakerOptions = computed<{ value: number; label: string }[]>(() => {
  const choice = config.value?.diarizer ?? "auto";
  const cap = choice === "nemo" ? 4 : 10;
  const opts: { value: number; label: string }[] = [{ value: 0, label: "Auto" }];
  for (let i = 1; i <= cap; i++) opts.push({ value: i, label: String(i) });
  return opts;
});

const languageOptions = computed(() => {
  const m = selectedAsrModel.value;
  const base = !m || !m.languages || !m.languages.length ? allLanguageOptions : m.languages;
  return base.includes("auto") ? base : ["auto", ...base];
});

function syncEngineAndModel(next: Config, preferEngine = false) {
  const installed = asrModels.value;
  if (!installed.length) return;

  if (preferEngine) {
    const engineModel = installed.find((m) => m.engine === next.engine);
    if (engineModel && !installed.some((m) => m.id === next.model && m.engine === next.engine)) {
      next.model = engineModel.id;
    }
    return;
  }

  const model = installed.find((m) => m.id === next.model);
  if (model) {
    if (next.engine !== model.engine) next.engine = model.engine as Config["engine"];
    return;
  }

  const engineModel = installed.find((m) => m.engine === next.engine);
  const fallback = engineModel ?? installed.find((m) => m.default_active) ?? installed[0];
  next.engine = fallback.engine as Config["engine"];
  next.model = fallback.id;
}

function onModelChanged() {
  if (!config.value) return;
  syncEngineAndModel(config.value);
  const opts = languageOptions.value;
  if (opts.length && !opts.includes(config.value.language)) {
    config.value.language = opts.includes("auto") ? "auto" : opts[0];
  }
}

const selectedEntry = computed<DirEntry | null>(() => {
  if (!listing.value || !selectedPath.value) return null;
  return listing.value.entries.find((e) => e.path === selectedPath.value) ?? null;
});

const audioEntries = computed<DirEntry[]>(() =>
  listing.value ? listing.value.entries.filter((e) => e.is_audio) : [],
);

const untranscribedEntries = computed<DirEntry[]>(() =>
  audioEntries.value.filter((e) => !e.cache_key && !busy.value[e.path]),
);

async function reload() {
  config.value = await api.loadConfig();
  models.value = await api.listModels();
  syncEngineAndModel(config.value);
  if (!listing.value) {
    const start = config.value.last_dir || (await api.defaultDir());
    await openDir(start);
  } else {
    await refreshListing();
  }
}

async function refreshListing() {
  if (!listing.value) return;
  try {
    listing.value = await api.listDirectory(listing.value.path);
  } catch (e) {
    error.value = String(e);
  }
}

async function openDir(path: string) {
  try {
    listing.value = await api.listDirectory(path);
    selectedPath.value = "";
    if (config.value && config.value.last_dir !== listing.value.path) {
      config.value.last_dir = listing.value.path;
    }
  } catch (e) {
    error.value = String(e);
  }
}

async function pickFolder() {
  const selected = await withDialog(() => open({ directory: true, multiple: false }));
  if (typeof selected === "string") void openDir(selected);
}

const audioExtensions = ["wav", "mp3", "ogg", "m4a", "flac", "opus", "webm", "aac", "wma"];

async function pickAudio() {
  const selected = await withDialog(() =>
    open({
      multiple: true,
      filters: [{ name: "Audio", extensions: audioExtensions }],
    }),
  );
  if (!selected) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  await addPathsToWorkdir(paths);
}

function basenameOf(p: string): string {
  const cleaned = p.replace(/\\/g, "/");
  const idx = cleaned.lastIndexOf("/");
  const tail = idx === -1 ? cleaned : cleaned.slice(idx + 1);
  return tail.split("?")[0] || "audio";
}

async function addPathsToWorkdir(paths: string[]) {
  if (!listing.value) return;
  const dir = listing.value.path;
  let lastAdded = "";
  for (const p of paths) {
    if (!hasAudioExt(p)) continue;
    try {
      lastAdded = await api.addToWorkdir(p, dir);
    } catch (eRaw) {
      try {
        const bytes = await readFile(p);
        lastAdded = await api.saveRecording(dir, basenameOf(p), bytes);
      } catch (e2) {
        error.value = `${eRaw} / ${e2}`;
      }
    }
  }
  await refreshListing();
  if (lastAdded) selectedPath.value = lastAdded;
}

function hasAudioExt(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return audioExtensions.includes(ext);
}

async function onRecordingSaved(path: string) {
  await refreshListing();
  selectedPath.value = path;
  transcript.value = null;
  error.value = null;
}

function chooseEntry(entry: DirEntry) {
  if (entry.path === selectedPath.value) return;
  selectedPath.value = entry.path;
  transcript.value = null;
  error.value = null;
  if (entry.cache_key) {
    void loadCached(entry.cache_key);
  }
}

async function loadCached(key: string) {
  try {
    const t = await api.historyLoad(key);
    if (t) transcript.value = t;
  } catch (e) {
    error.value = String(e);
  }
}

onMounted(async () => {
  version.value = await api.appVersion();
  sys.value = await api.systemInfo();
  await reload();
  if (config.value && sys.value && !sys.value.cuda_available && config.value.device === "cuda") {
    config.value.device = "cpu";
  }
  if (config.value && sys.value?.is_mobile && config.value.diarizer === "nemo") {
    config.value.diarizer = "auto";
  }
  if (config.value && (config.value.diarizer as string) === "sherpa") {
    config.value.diarizer = "eres2net";
  }
  unlistenProgress = await events.onTranscribeProgress((p) => {
    progressByPath.value = { ...progressByPath.value, [p.path]: p };
  });
  try {
    essentialIds.value = await api.essentialModels();
  } catch {
    essentialIds.value = [];
  }
  const refreshModels = async (id?: string) => {
    models.value = await api.listModels();
    if (id && error.value && error.value.includes("not installed") && config.value?.model === id) {
      error.value = null;
    }
  };
  unlistenModelProgress = await events.onModelProgress((p) => {
    if (essentialIds.value.includes(p.id)) {
      essentialProgress.value = { ...essentialProgress.value, [p.id]: p };
    }
  });
  unlistenModelDone = await events.onModelDone((id) => {
    if (id) {
      const next = { ...essentialErrors.value };
      delete next[id];
      essentialErrors.value = next;
    }
    void refreshModels(id);
  });
  unlistenModelError = await events.onModelError((id) => {
    if (id) essentialErrors.value = { ...essentialErrors.value, [id]: true };
    void refreshModels();
  });
  unlistenEssentialsDone = await events.onEssentialsDone(() => {
    essentialsForceReady.value = true;
    void refreshModels();
  });
  unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
    if (tab.value !== "transcribe") return;
    if (event.payload.type === "over") dragOver.value = true;
    else if (event.payload.type === "leave") dragOver.value = false;
    else if (event.payload.type === "drop") {
      dragOver.value = false;
      const paths = event.payload.paths ?? [];
      if (paths.length) void addPathsToWorkdir(paths);
    }
  });
});

let unlistenDrop: (() => void) | null = null;
let unlistenProgress: (() => void) | null = null;
let unlistenModelDone: (() => void) | null = null;
let unlistenModelError: (() => void) | null = null;
let unlistenModelProgress: (() => void) | null = null;
let unlistenEssentialsDone: (() => void) | null = null;
onUnmounted(() => {
  unlistenDrop?.();
  unlistenProgress?.();
  unlistenModelDone?.();
  unlistenModelError?.();
  unlistenModelProgress?.();
  unlistenEssentialsDone?.();
});

watch(tab, (t) => {
  if (t === "transcribe") void refreshListing();
});

let saveTimer: ReturnType<typeof setTimeout> | null = null;
watch(
  config,
  (next) => {
    if (!next) return;
    if (saveTimer) clearTimeout(saveTimer);
    saveState.value = "saving";
    saveTimer = setTimeout(async () => {
      try {
        await api.saveConfig(next);
        saveState.value = "saved";
        setTimeout(() => {
          if (saveState.value === "saved") saveState.value = "idle";
        }, 1500);
      } catch (e) {
        error.value = `save failed: ${String(e)}`;
        saveState.value = "idle";
      }
    }, 250);
  },
  { deep: true },
);

async function runTranscribe(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || !config.value) return;
  if (!target.is_audio) return;
  if (!selectedModelInstalled.value) {
    error.value = `Model "${selectedAsrModel.value?.display_name ?? config.value.model}" is not installed. Download it in Configuration.`;
    tab.value = "transcribe";
    return;
  }
  selectedPath.value = target.path;
  status.value = "running";
  error.value = null;
  busy.value = { ...busy.value, [target.path]: true };
  try {
    transcript.value = await api.transcribeFile(target.path, config.value);
    status.value = "idle";
    await refreshListing();
  } catch (e) {
    const msg = String(e);
    if (msg.includes("cancelled")) {
      status.value = "idle";
    } else {
      error.value = msg;
      status.value = "error";
    }
  } finally {
    const next = { ...busy.value };
    delete next[target.path];
    busy.value = next;
    const np = { ...progressByPath.value };
    delete np[target.path];
    progressByPath.value = np;
  }
}

async function stopTranscribe(entry: DirEntry) {
  await api.cancelTranscribe(entry.path);
}

async function transcribeAll() {
  if (!config.value || queueActive.value) return;
  const items = untranscribedEntries.value;
  if (!items.length) return;
  queueActive.value = true;
  queueTotal.value = items.length;
  queueDone.value = 0;
  try {
    for (const entry of items) {
      if (!config.value) break;
      await runTranscribe(entry);
      queueDone.value += 1;
    }
  } finally {
    queueActive.value = false;
    queueTotal.value = 0;
    queueDone.value = 0;
  }
}

async function autoRename(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || !target.is_audio) return;
  let t = transcript.value;
  if (!t) {
    if (target.cache_key) {
      t = await api.historyLoad(target.cache_key);
    }
  }
  if (!t) {
    await message("Transcribe first to enable auto-rename.", {
      title: "Auto-rename",
      kind: "info",
    });
    return;
  }
  status.value = "renaming";
  try {
    const s = await api.suggestFilename(t);
    const ext = target.name.includes(".") ? target.name.split(".").pop() : "";
    const suggestion = `${s.topic}_${s.stamp}${ext ? "." + ext : ""}`;
    const ok = await withDialog(() =>
      confirm(`Rename to:\n\n${suggestion}`, { title: "Auto-rename", okLabel: "Rename" }),
    );
    if (!ok) return;
    const newPath = await api.renameFile(target.path, suggestion);
    selectedPath.value = newPath;
    await refreshListing();
  } catch (e) {
    error.value = `auto-rename failed: ${String(e)}`;
  } finally {
    status.value = "idle";
  }
}

const renaming = ref(false);
const renameTarget = ref<DirEntry | null>(null);
const renameValue = ref("");

function openRename(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target) return;
  if (renaming.value || exporting.value) return;
  renameTarget.value = target;
  renameValue.value = target.name;
  renaming.value = true;
}

async function commitRename() {
  if (!renameTarget.value) return;
  const target = renameTarget.value;
  const next = renameValue.value.trim();
  renaming.value = false;
  if (!next || next === target.name) return;
  try {
    const newPath = await api.renameFile(target.path, next);
    if (selectedPath.value === target.path) selectedPath.value = newPath;
    await refreshListing();
  } catch (e) {
    error.value = String(e);
  }
}

async function deleteEntry(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target) return;
  const ok = await withDialog(() =>
    confirm(`Delete "${target.name}"?\n\nThis cannot be undone.`, {
      title: "Delete file",
      okLabel: "Delete",
      kind: "warning",
    }),
  );
  if (!ok) return;
  try {
    await api.deleteFile(target.path);
    if (selectedPath.value === target.path) {
      selectedPath.value = "";
      transcript.value = null;
    }
    await refreshListing();
  } catch (e) {
    error.value = String(e);
  }
}

const exporting = ref(false);
const exportTarget = ref<DirEntry | null>(null);
const exportFormat = ref<ExportFormat>("txt");

const TRANSCRIPT_HEIGHT_KEY = "wt.transcriptHeightVh";
const transcriptHeightVh = ref(
  (() => {
    const v = Number(localStorage.getItem(TRANSCRIPT_HEIGHT_KEY) ?? "");
    return Number.isFinite(v) && v >= 20 && v <= 80 ? v : 40;
  })(),
);
watch(transcriptHeightVh, (v) => {
  localStorage.setItem(TRANSCRIPT_HEIGHT_KEY, String(Math.round(v)));
});
const resizingTranscript = ref(false);
function beginTranscriptResize(ev: PointerEvent) {
  ev.preventDefault();
  resizingTranscript.value = true;
  const startY = ev.clientY;
  const startVh = transcriptHeightVh.value;
  const move = (e: PointerEvent) => {
    const deltaPx = startY - e.clientY;
    const deltaVh = (deltaPx / window.innerHeight) * 100;
    transcriptHeightVh.value = Math.max(20, Math.min(80, startVh + deltaVh));
  };
  const up = () => {
    resizingTranscript.value = false;
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}

function drawWaveformFallback(canvas: HTMLCanvasElement) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const w = canvas.width;
  const h = canvas.height;
  ctx.clearRect(0, 0, w, h);
  ctx.strokeStyle = "#6750a4";
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(0, h / 2);
  ctx.lineTo(w, h / 2);
  ctx.stroke();
}

function drawWaveformPeaks(canvas: HTMLCanvasElement, peaks: number[], startFrac = 0, endFrac = 1) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  const step = canvas.width / peaks.length;
  peaks.forEach((peak, i) => {
    const amp = Math.max(0.02, Math.min(1, peak));
    const barHeight = amp * canvas.height;
    const x = i * step;
    const y = (canvas.height - barHeight) / 2;
    const frac = (i + 0.5) / peaks.length;
    const inside = frac >= startFrac && frac <= endFrac;
    ctx.fillStyle = inside ? "#6750a4" : "rgba(103, 80, 164, 0.22)";
    ctx.fillRect(x, y, Math.max(1, step - 1), barHeight);
  });
}

const trimming = ref(false);
const trimTarget = ref<DirEntry | null>(null);
const trimDuration = ref(0);
const trimStart = ref(0);
const trimEnd = ref(0);
const trimPeaks = ref<number[]>([]);
const trimCanvas = ref<HTMLCanvasElement | null>(null);
const trimAudioSrc = ref("");
const trimError = ref<string | null>(null);
const trimLoading = ref(false);

const trimStartFrac = computed(() =>
  trimDuration.value > 0 ? trimStart.value / trimDuration.value : 0,
);
const trimEndFrac = computed(() =>
  trimDuration.value > 0 ? trimEnd.value / trimDuration.value : 1,
);

const loadingTrimPath = ref<string | null>(null);

function openTrim(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || !target.is_audio) return;
  if (trimming.value || renaming.value || exporting.value) return;
  if (loadingTrimPath.value) return;
  loadingTrimPath.value = target.path;
  trimError.value = null;
  if (trimAudioSrc.value.startsWith("blob:")) URL.revokeObjectURL(trimAudioSrc.value);
  trimAudioSrc.value = "";
  trimPeaks.value = [];
  setTimeout(() => void doOpenTrim(target), 0);
}

async function doOpenTrim(target: DirEntry) {
  let nextDur = 0;
  let nextStart = 0;
  let nextEnd = 0;
  let nextPeaks: number[] = [];
  try {
    const [durMs, meta, peaks] = await Promise.all([
      api.probeAudio(target.path).catch(() => null),
      api.loadAudioMeta(target.path),
      api.audioWaveform(target.path, 320),
    ]);
    nextDur = Math.max(0, Math.floor((durMs ?? target.duration_ms ?? 0) as number));
    nextStart = Math.min(meta.trim_start_ms ?? 0, nextDur);
    nextEnd = Math.min(meta.trim_end_ms ?? nextDur, nextDur);
    if (nextEnd <= nextStart) nextEnd = nextDur;
    nextPeaks = peaks;
  } catch (e) {
    loadingTrimPath.value = null;
    error.value = `prepare: ${String(e)}`;
    return;
  }
  trimTarget.value = target;
  trimAudioSrc.value = "";
  trimDuration.value = nextDur;
  trimStart.value = nextStart;
  trimEnd.value = nextEnd;
  trimPlayOffsetMs = nextStart;
  trimAudioBuffer = null;
  trimAudioPath = null;
  trimPeaks.value = nextPeaks;
  trimLoading.value = false;
  trimming.value = true;
  loadingTrimPath.value = null;
  await renderTrimWaveform();
}

async function renderTrimWaveform() {
  await nextTick();
  const canvas = trimCanvas.value;
  if (!canvas) return;
  if (trimPeaks.value.length === 0) {
    drawWaveformFallback(canvas);
    return;
  }
  drawWaveformPeaks(canvas, trimPeaks.value, trimStartFrac.value, trimEndFrac.value);
}

watch([trimStart, trimEnd, trimPeaks], () => {
  if (trimming.value) void renderTrimWaveform();
});

const waveformBox = ref<HTMLElement | null>(null);
const MIN_TRIM_GAP_MS = 3_000;
const trimPlaying = ref(false);
const trimPlayheadMs = ref(0);
const trimPlayheadFrac = computed(() =>
  trimDuration.value > 0
    ? Math.max(0, Math.min(1, trimPlayheadMs.value / trimDuration.value))
    : 0,
);
let trimPlayheadRaf: number | null = null;
function stopPlayheadLoop() {
  if (trimPlayheadRaf !== null) cancelAnimationFrame(trimPlayheadRaf);
  trimPlayheadRaf = null;
}
function startPlayheadLoop() {
  stopPlayheadLoop();
  const tick = () => {
    if (!trimPlaying.value || !trimAudioCtx) {
      trimPlayheadRaf = null;
      return;
    }
    const ms = trimPlayOffsetMs + (trimAudioCtx.currentTime - trimPlayStartCtx) * 1000;
    trimPlayheadMs.value = Math.min(trimEnd.value, Math.max(trimStart.value, ms));
    trimPlayheadRaf = requestAnimationFrame(tick);
  };
  trimPlayheadRaf = requestAnimationFrame(tick);
}
const trimAudioLoading = ref(false);
let trimAudioCtx: AudioContext | null = null;
let trimAudioBuffer: AudioBuffer | null = null;
let trimAudioSource: AudioBufferSourceNode | null = null;
let trimPlayStartCtx = 0;
let trimPlayOffsetMs = 0;
let trimAudioPath: string | null = null;
let trimStopTimer: ReturnType<typeof setTimeout> | null = null;

async function ensureTrimBuffer(): Promise<AudioBuffer | null> {
  const target = trimTarget.value;
  if (!target) return null;
  if (trimAudioBuffer && trimAudioPath === target.path) return trimAudioBuffer;
  trimAudioLoading.value = true;
  try {
    const bytes = await api.readAudioBytes(target.path);
    if (!trimAudioCtx) trimAudioCtx = new AudioContext();
    const ab = new Uint8Array(bytes).buffer;
    trimAudioBuffer = await trimAudioCtx.decodeAudioData(ab);
    trimAudioPath = target.path;
    return trimAudioBuffer;
  } finally {
    trimAudioLoading.value = false;
  }
}

function clearTrimSource() {
  if (trimAudioSource) {
    try {
      trimAudioSource.onended = null;
      trimAudioSource.stop();
    } catch {}
    try {
      trimAudioSource.disconnect();
    } catch {}
    trimAudioSource = null;
  }
  if (trimStopTimer) clearTimeout(trimStopTimer);
  trimStopTimer = null;
}

function stopTrimPlay() {
  clearTrimSource();
  trimPlayOffsetMs = trimStart.value;
  trimPlayheadMs.value = trimStart.value;
  trimPlaying.value = false;
  stopPlayheadLoop();
}

function pauseTrimPlay() {
  if (trimPlaying.value && trimAudioCtx) {
    const elapsed = (trimAudioCtx.currentTime - trimPlayStartCtx) * 1000;
    trimPlayOffsetMs = Math.min(trimEnd.value, trimPlayOffsetMs + elapsed);
    trimPlayheadMs.value = trimPlayOffsetMs;
  }
  clearTrimSource();
  trimPlaying.value = false;
  stopPlayheadLoop();
}

async function toggleTrimPlay() {
  if (trimPlaying.value) {
    pauseTrimPlay();
    return;
  }
  const buffer = await ensureTrimBuffer().catch((e) => {
    trimError.value = `load: ${String(e)}`;
    return null;
  });
  if (!buffer || !trimAudioCtx) return;
  if (trimAudioCtx.state === "suspended") await trimAudioCtx.resume();
  let offsetMs = trimPlayOffsetMs;
  if (offsetMs < trimStart.value || offsetMs >= trimEnd.value) offsetMs = trimStart.value;
  const durMs = Math.max(0, trimEnd.value - offsetMs);
  const src = trimAudioCtx.createBufferSource();
  src.buffer = buffer;
  src.connect(trimAudioCtx.destination);
  src.onended = () => {
    if (trimAudioSource === src) {
      trimAudioSource = null;
      trimPlayOffsetMs = trimStart.value;
      trimPlayheadMs.value = trimStart.value;
      trimPlaying.value = false;
      stopPlayheadLoop();
    }
  };
  trimAudioSource = src;
  trimPlayStartCtx = trimAudioCtx.currentTime;
  trimPlayOffsetMs = offsetMs;
  src.start(0, offsetMs / 1000, durMs / 1000);
  trimPlayheadMs.value = offsetMs;
  trimPlaying.value = true;
  startPlayheadLoop();
  trimStopTimer = setTimeout(() => {
    if (trimAudioSource === src) clearTrimSource();
    trimPlayOffsetMs = trimStart.value;
    trimPlayheadMs.value = trimStart.value;
    trimPlaying.value = false;
    stopPlayheadLoop();
  }, durMs + 200);
}

function beginHandleDrag(side: "start" | "end", ev: PointerEvent) {
  ev.preventDefault();
  ev.stopPropagation();
  const box = waveformBox.value;
  if (!box || trimDuration.value === 0) return;
  const dur = trimDuration.value;
  const gap = Math.min(MIN_TRIM_GAP_MS, Math.max(100, Math.floor(dur / 4)));
  const move = (e: PointerEvent) => {
    const rect = box.getBoundingClientRect();
    const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
    const ms = Math.round((x / rect.width) * dur);
    if (side === "start") {
      trimStart.value = Math.max(0, Math.min(ms, trimEnd.value - gap));
    } else {
      trimEnd.value = Math.min(dur, Math.max(ms, trimStart.value + gap));
    }
  };
  const up = (e: PointerEvent) => {
    move(e);
    window.removeEventListener("pointermove", move);
    window.removeEventListener("pointerup", up);
    window.removeEventListener("pointercancel", up);
  };
  window.addEventListener("pointermove", move);
  window.addEventListener("pointerup", up);
  window.addEventListener("pointercancel", up);
}

function resetTrim() {
  trimStart.value = 0;
  trimEnd.value = trimDuration.value;
}

async function commitTrim() {
  if (!trimTarget.value) return;
  const target = trimTarget.value;
  const start = Math.max(0, Math.floor(trimStart.value));
  const end = Math.min(trimDuration.value, Math.floor(trimEnd.value));
  const meta: AudioMeta = {
    trim_start_ms: start,
    trim_end_ms: end < trimDuration.value ? end : null,
  };
  if (start === 0) meta.trim_start_ms = 0;
  try {
    await api.saveAudioMeta(target.path, meta);
    stopTrimPlay();
    trimAudioBuffer = null;
    trimAudioPath = null;
    trimPlayOffsetMs = 0;
    trimming.value = false;
    trimTarget.value = null;
    trimAudioSrc.value = "";
    await refreshListing();
  } catch (e) {
    trimError.value = String(e);
  }
}

function closeTrim() {
  stopTrimPlay();
  trimAudioBuffer = null;
  trimAudioPath = null;
  trimPlayOffsetMs = 0;
  trimming.value = false;
  trimTarget.value = null;
  trimAudioSrc.value = "";
  trimError.value = null;
}

async function openExport(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target) return;
  if (renaming.value || exporting.value) return;
  let t = transcript.value;
  if (!t || (selectedEntry.value && selectedEntry.value.path !== target.path)) {
    if (target.cache_key) t = await api.historyLoad(target.cache_key);
  }
  if (!t) {
    await message("Transcribe this file first to enable export.", {
      title: "Export",
      kind: "info",
    });
    return;
  }
  exportTarget.value = target;
  exporting.value = true;
}

async function commitExport() {
  if (!exportTarget.value) return;
  const target = exportTarget.value;
  const fmt = exportFormat.value;
  exporting.value = false;
  let t = transcript.value;
  if (!t && target.cache_key) t = await api.historyLoad(target.cache_key);
  if (!t) return;
  const stem = target.name.replace(/\.[^.]+$/, "");
  const dest = await withDialog(() =>
    save({
      defaultPath: `${stem}.${fmt}`,
      filters: [{ name: fmt.toUpperCase(), extensions: [fmt] }],
    }),
  );
  if (!dest) return;
  try {
    await api.exportTranscript(t, dest, fmt);
  } catch (e) {
    error.value = String(e);
  }
}

function fmt(ms: number): string {
  const s = Math.floor(ms / 1000);
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
}

function fmtLong(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const r = s % 60;
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(r).padStart(2, "0")}`;
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

const fieldClass =
  "w-full bg-surface-container-high border border-outline-variant/60 text-on-surface text-bodyMedium px-md py-xs rounded-lg appearance-none focus:outline-none focus:border-primary transition-colors";
</script>

<template>
  <SetupGate
    v-if="!essentialsReady"
    :essential-ids="essentialIds"
    :models="models"
    :progress="essentialProgress"
    :errors="essentialErrors"
  />
  <div class="h-full flex flex-col bg-background text-on-background overflow-hidden">
    <header
      class="flex justify-between items-center w-full px-margin h-14 md:h-16 shrink-0 border-b border-outline-variant/40 bg-surface gap-xs"
    >
      <div class="flex items-center gap-xs">
        <span class="material-symbols-outlined text-primary text-[24px]">graphic_eq</span>
        <span
          class="font-mono tracking-tighter font-bold text-primary text-labelMedium ml-xs uppercase"
          >wt</span
        >
      </div>
      <nav
        class="hidden md:flex items-center gap-md md:gap-xl h-full overflow-x-auto scroll-thin min-w-0"
      >
        <button
          v-for="t in tabs"
          :key="t.id"
          @click="tab = t.id"
          class="h-full flex items-center text-titleSmall border-b-2 px-unit transition-colors whitespace-nowrap shrink-0"
          :class="
            tab === t.id
              ? 'border-primary text-on-surface'
              : 'border-transparent text-on-surface-variant hover:text-on-surface'
          "
        >
          {{ t.label }}
        </button>
      </nav>
      <button
        class="flex items-center justify-center w-11 h-11 -mr-xs text-on-surface-variant shrink-0 gap-xs"
      >
        <span class="font-mono text-labelSmall hidden sm:inline">v{{ version }}</span>
        <span class="material-symbols-outlined text-[22px]">more_vert</span>
      </button>
    </header>

    <main class="flex-1 flex flex-col md:flex-row overflow-hidden min-h-0" @click="closeMenus">
      <template v-if="tab === 'transcribe'">
        <section
          class="flex-1 flex flex-col overflow-hidden bg-surface relative"
          :class="dragOver ? 'ring-2 ring-primary ring-inset' : ''"
        >
          <div
            class="flex items-center gap-xs px-margin h-14 md:h-12 border-b border-outline-variant/40 shrink-0 overflow-x-auto md:overflow-visible scroll-thin"
          >
            <span
              class="material-symbols-outlined text-on-surface-variant text-[20px] md:text-[18px] shrink-0"
              >folder</span
            >
            <span
              class="font-mono text-labelMedium text-on-surface truncate min-w-0 hidden sm:inline"
              :title="listing?.path"
            >
              {{ listing?.path ?? "—" }}
            </span>
            <button
              class="text-on-surface-variant hover:text-on-surface transition-colors w-11 h-11 md:w-auto md:h-auto flex items-center justify-center shrink-0"
              @click="refreshListing"
              title="Refresh"
            >
              <span class="material-symbols-outlined text-[22px] md:text-[18px]">refresh</span>
            </button>
            <div class="flex-1"></div>
            <div
              v-if="queueActive"
              class="font-mono text-labelSmall text-secondary flex items-center gap-unit"
            >
              <span class="w-1.5 h-1.5 rounded-full bg-secondary animate-pulse"></span>
              queue {{ queueDone + 1 }}/{{ queueTotal }}
            </div>
            <button
              class="min-w-11 h-11 md:h-auto px-md md:py-unit rounded-full border border-outline-variant text-on-surface text-labelMedium hover:bg-surface-container-high transition-colors flex items-center justify-center gap-unit shrink-0 whitespace-nowrap disabled:opacity-40 disabled:cursor-not-allowed"
              :disabled="queueActive || untranscribedEntries.length === 0"
              @click="transcribeAll"
              title="Transcribe every untranscribed audio file in this folder"
            >
              <span class="material-symbols-outlined text-[20px] md:text-[16px]"
                >playlist_play</span
              >
              <span class="hidden sm:inline">Transcribe all</span>
            </button>
            <button
              class="min-w-11 h-11 md:h-auto px-md md:py-unit rounded-full border border-outline-variant text-on-surface text-labelMedium hover:bg-surface-container-high transition-colors flex items-center justify-center gap-unit shrink-0 whitespace-nowrap"
              @click="pickFolder"
              title="Change working folder"
            >
              <span class="material-symbols-outlined text-[20px] md:text-[16px]">folder_open</span>
              <span class="hidden sm:inline">Change</span>
            </button>
            <button
              class="min-w-11 h-11 md:h-auto px-md md:py-unit rounded-full bg-primary text-on-primary text-labelMedium hover:bg-primary-fixed-dim transition-colors flex items-center justify-center gap-unit shrink-0 whitespace-nowrap"
              @click="pickAudio"
              title="Add audio file(s) to working folder"
            >
              <span class="material-symbols-outlined text-[20px] md:text-[16px]">add</span>
              <span class="hidden sm:inline">Add audio</span>
            </button>
          </div>

          <div
            v-if="error"
            class="m-margin p-md rounded-lg bg-error-container/30 border border-error/40 text-error text-bodyMedium flex items-start gap-xs"
          >
            <span class="material-symbols-outlined text-[18px] mt-[1px] shrink-0">error</span>
            <span class="flex-1 break-words font-mono text-labelMedium">{{ error }}</span>
            <button
              class="text-titleSmall underline hover:opacity-80 shrink-0"
              @click="tab = 'logs'"
            >
              View log
            </button>
            <button
              class="material-symbols-outlined text-[18px] hover:opacity-70"
              @click="error = null"
            >
              close
            </button>
          </div>

          <div class="flex-1 overflow-y-auto scroll-thin">
            <div
              v-if="!listing || audioEntries.length === 0"
              class="h-full flex flex-col items-center justify-center gap-md text-center px-xl text-on-surface-variant"
            >
              <span
                class="material-symbols-outlined text-[48px]"
                :class="dragOver ? 'text-primary' : 'text-outline-variant'"
              >
                {{ dragOver ? "download" : "library_music" }}
              </span>
              <p class="text-bodyMedium">
                {{ dragOver ? "Drop to add" : "No audio in this folder" }}
              </p>
              <p class="font-mono text-labelSmall text-outline">
                Drag files here or click Add audio
              </p>
            </div>

            <ul v-else class="flex flex-col md:hidden">
              <li
                v-for="entry in audioEntries"
                :key="`m-${entry.path}`"
                class="border-b border-outline-variant/20 px-margin py-md cursor-pointer transition-colors"
                :class="selectedPath === entry.path ? 'bg-primary/10' : ''"
                @click="chooseEntry(entry)"
              >
                <div class="flex items-start gap-xs">
                  <div class="flex-1 min-w-0">
                    <div class="flex items-start gap-xs">
                      <div
                        class="flex-1 min-w-0 text-bodyMedium text-on-surface break-words"
                        :title="entry.name"
                      >
                        {{ prettyName(entry.name).display }}
                      </div>
                      <div class="flex items-center gap-unit shrink-0 -mt-unit -mr-xs" @click.stop>
                        <button
                          v-if="busy[entry.path]"
                          class="material-symbols-outlined w-10 h-10 flex items-center justify-center rounded-full text-error hover:bg-error-container/40 transition-colors"
                          title="Stop"
                          @click="stopTranscribe(entry)"
                        >
                          stop
                        </button>
                        <button
                          v-else
                          class="w-10 h-10 flex items-center justify-center rounded-full text-primary hover:bg-surface-container-highest transition-colors"
                          title="Transcribe"
                          @click="runTranscribe(entry)"
                        >
                          <TranscribeIcon :size="20" />
                        </button>
                        <div class="relative">
                          <button
                            class="material-symbols-outlined w-10 h-10 flex items-center justify-center rounded-full text-on-surface-variant hover:bg-surface-container-highest transition-colors"
                            title="More"
                            @click="toggleMenu(entry.path)"
                          >
                            more_vert
                          </button>
                          <div
                            v-if="openMenuPath === entry.path"
                            class="absolute right-0 top-full mt-unit z-30 min-w-[180px] bg-surface-container-high border border-outline-variant/60 rounded-lg shadow-2xl py-unit"
                          >
                            <button
                              class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-on-surface hover:bg-surface-container-highest transition-colors"
                              @click="
                                closeMenus();
                                openTrim(entry);
                              "
                            >
                              <span class="material-symbols-outlined text-[18px]">content_cut</span>
                              Cut / select range
                            </button>
                            <button
                              class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-on-surface hover:bg-surface-container-highest transition-colors"
                              @click="
                                closeMenus();
                                autoRename(entry);
                              "
                            >
                              <span class="material-symbols-outlined text-[18px]"
                                >auto_awesome</span
                              >
                              Auto-rename
                            </button>
                            <button
                              class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-on-surface hover:bg-surface-container-highest transition-colors"
                              @click="
                                closeMenus();
                                openRename(entry);
                              "
                            >
                              <span class="material-symbols-outlined text-[18px]"
                                >drive_file_rename_outline</span
                              >
                              Rename
                            </button>
                            <button
                              class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-on-surface hover:bg-surface-container-highest transition-colors disabled:opacity-30"
                              :disabled="!entry.cache_key"
                              @click="
                                closeMenus();
                                openExport(entry);
                              "
                            >
                              <span class="material-symbols-outlined text-[18px]">download</span>
                              Export
                            </button>
                            <div class="my-unit border-t border-outline-variant/40"></div>
                            <button
                              class="w-full px-md py-xs flex items-center gap-xs text-bodyMedium text-error hover:bg-error-container/40 transition-colors"
                              @click="
                                closeMenus();
                                deleteEntry(entry);
                              "
                            >
                              <span class="material-symbols-outlined text-[18px]">delete</span>
                              Delete
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
                <div
                  class="flex items-center flex-wrap gap-xs mt-xs font-mono text-labelSmall text-on-surface-variant"
                >
                  <span>{{ entry.duration_ms ? fmt(entry.duration_ms) : "—" }}</span>
                  <span class="text-outline">·</span>
                  <span>{{ fmtBytes(entry.size_bytes) }}</span>
                  <template v-if="busy[entry.path]">
                    <span class="text-outline">·</span>
                    <span class="text-secondary flex items-center gap-unit">
                      <span class="material-symbols-outlined text-[14px] animate-pulse"
                        >graphic_eq</span
                      >
                      <template v-if="progressByPath[entry.path]">
                        <span v-if="progressByPath[entry.path].phase === 'transcribing'"
                          >{{ progressByPath[entry.path].displayPct.toFixed(1) }}%</span
                        >
                        <span v-else>{{ phaseLabel(progressByPath[entry.path].phase) }}</span>
                      </template>
                      <span v-else>transcribing</span>
                    </span>
                  </template>
                  <template v-else-if="entry.cache_key">
                    <span class="text-outline">·</span>
                    <span class="text-tertiary flex items-center gap-unit">
                      <span class="material-symbols-outlined text-[14px]">check_circle</span>
                      transcribed
                    </span>
                  </template>
                  <span v-if="prettyName(entry.name).timestamp" class="ml-auto text-secondary">{{
                    prettyName(entry.name).timestamp
                  }}</span>
                </div>
              </li>
            </ul>

            <table v-if="audioEntries.length" class="hidden md:table w-full text-bodyMedium">
              <thead class="sticky top-0 bg-surface z-10 border-b border-outline-variant/40">
                <tr
                  class="text-left font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide"
                >
                  <th class="px-margin py-xs w-8"></th>
                  <th class="px-xs py-xs">Name</th>
                  <th class="px-xs py-xs w-24">Duration</th>
                  <th class="px-xs py-xs w-24">Size</th>
                  <th class="px-xs py-xs w-28">Status</th>
                  <th class="px-margin py-xs w-[200px]"></th>
                </tr>
              </thead>
              <tbody>
                <tr
                  v-for="entry in audioEntries"
                  :key="entry.path"
                  class="border-b border-outline-variant/20 hover:bg-surface-container-high/40 cursor-pointer transition-colors"
                  :class="selectedPath === entry.path ? 'bg-primary/10' : ''"
                  @click="chooseEntry(entry)"
                  @dblclick="runTranscribe(entry)"
                >
                  <td class="px-margin py-xs">
                    <span class="material-symbols-outlined text-[20px] text-on-surface-variant"
                      >graphic_eq</span
                    >
                  </td>
                  <td class="px-xs py-xs truncate max-w-0">
                    <span class="text-on-surface" :title="entry.name">{{
                      prettyName(entry.name).display
                    }}</span>
                    <span
                      v-if="prettyName(entry.name).timestamp"
                      class="font-mono text-labelSmall text-secondary ml-xs"
                      >{{ prettyName(entry.name).timestamp }}</span
                    >
                  </td>
                  <td class="px-xs py-xs font-mono text-labelMedium text-on-surface-variant">
                    {{ entry.duration_ms ? fmt(entry.duration_ms) : "—" }}
                  </td>
                  <td class="px-xs py-xs font-mono text-labelMedium text-on-surface-variant">
                    {{ fmtBytes(entry.size_bytes) }}
                  </td>
                  <td class="px-xs py-xs">
                    <template v-if="busy[entry.path]">
                      <div class="flex flex-col gap-unit">
                        <span
                          class="font-mono text-labelSmall text-secondary flex items-center gap-unit"
                        >
                          <span class="material-symbols-outlined text-[14px] animate-pulse"
                            >graphic_eq</span
                          >
                          <template v-if="progressByPath[entry.path]">
                            <span v-if="progressByPath[entry.path].phase === 'transcribing'">
                              {{ progressByPath[entry.path].displayPct.toFixed(1) }}%
                            </span>
                            <span v-else>{{ phaseLabel(progressByPath[entry.path].phase) }}</span>
                          </template>
                          <span v-else>transcribing</span>
                        </span>
                      </div>
                    </template>
                    <span
                      v-else-if="entry.cache_key"
                      class="font-mono text-labelSmall text-tertiary flex items-center gap-unit"
                    >
                      <span class="material-symbols-outlined text-[14px]">check_circle</span>
                      transcribed
                    </span>
                    <span v-else class="font-mono text-labelSmall text-outline">—</span>
                  </td>
                  <td class="px-margin py-xs text-right">
                    <div class="inline-flex gap-unit" @click.stop>
                      <button
                        v-if="busy[entry.path]"
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-error-container/40 text-error transition-colors"
                        title="Stop transcription"
                        @click="stopTranscribe(entry)"
                      >
                        stop
                      </button>
                      <button
                        v-else
                        class="p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-primary transition-colors"
                        title="Transcribe"
                        @click="runTranscribe(entry)"
                      >
                        <TranscribeIcon :size="18" />
                      </button>
                      <button
                        class="p-unit rounded hover:bg-surface-container-highest text-on-surface-variant transition-colors"
                        :class="
                          entry.trim_start_ms || entry.trim_end_ms
                            ? 'text-primary'
                            : 'hover:text-primary'
                        "
                        :title="
                          entry.trim_start_ms || entry.trim_end_ms
                            ? `Trim: ${fmt(entry.trim_start_ms ?? 0)} – ${fmt(
                                entry.trim_end_ms ?? entry.duration_ms ?? 0,
                              )}`
                            : 'Trim — select range to transcribe'
                        "
                        :disabled="loadingTrimPath === entry.path"
                        @click="openTrim(entry)"
                      >
                        <Spinner v-if="loadingTrimPath === entry.path" :size="18" />
                        <span v-else class="material-symbols-outlined text-[18px]"
                          >content_cut</span
                        >
                      </button>
                      <button
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-secondary transition-colors"
                        title="Auto-rename (AI)"
                        @click="autoRename(entry)"
                      >
                        auto_awesome
                      </button>
                      <button
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-on-surface transition-colors"
                        title="Rename"
                        @click="openRename(entry)"
                      >
                        drive_file_rename_outline
                      </button>
                      <button
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-on-surface transition-colors"
                        title="Export transcript"
                        :disabled="!entry.cache_key"
                        :class="!entry.cache_key ? 'opacity-30 cursor-not-allowed' : ''"
                        @click="openExport(entry)"
                      >
                        download
                      </button>
                      <button
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-error-container/40 text-on-surface-variant hover:text-error transition-colors"
                        title="Delete"
                        @click="deleteEntry(entry)"
                      >
                        delete
                      </button>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div
            v-if="transcript"
            class="relative shrink-0 border-t border-outline-variant/40 flex flex-col"
            :style="{ height: transcriptHeightVh + 'vh' }"
          >
            <div
              class="absolute -top-1 left-0 right-0 h-2 cursor-row-resize touch-none flex items-center justify-center group z-10"
              :class="resizingTranscript ? 'bg-primary/20' : ''"
              @pointerdown="beginTranscriptResize"
              title="Drag to resize transcript pane"
            >
              <div
                class="w-12 h-1 rounded-full transition-colors"
                :class="resizingTranscript ? 'bg-primary' : 'bg-outline-variant group-hover:bg-primary/60'"
              ></div>
            </div>
            <div class="flex-1 overflow-y-auto scroll-thin p-margin">
            <div class="flex items-center justify-between mb-md">
              <h3 class="text-titleSmall text-on-surface flex items-center gap-xs">
                <span class="material-symbols-outlined text-primary text-[18px]">subtitles</span>
                Transcript
                <span v-if="selectedPath" class="font-mono text-labelSmall text-on-surface-variant"
                  >— {{ basename(selectedPath) }}</span
                >
              </h3>
              <button
                class="text-on-surface-variant hover:text-on-surface text-titleSmall"
                @click="closeTranscript"
              >
                <span class="material-symbols-outlined text-[18px]">close</span>
              </button>
            </div>
            <article
              v-for="(u, i) in transcript.utterances"
              :key="i"
              class="flex gap-md items-start group hover:bg-surface-container-high/30 -mx-xs px-xs py-unit rounded transition-colors"
            >
              <span class="font-mono text-labelSmall text-secondary w-20 shrink-0 pt-unit">{{
                fmt(u.start_ms)
              }}</span>
              <div class="flex-1 min-w-0">
                <div v-if="u.speaker" class="font-mono text-labelSmall text-primary mb-unit">
                  {{ u.speaker }}
                </div>
                <p
                  class="text-bodyMedium text-on-surface-variant group-hover:text-on-surface transition-colors leading-relaxed"
                >
                  {{ u.text }}
                </p>
              </div>
            </article>
            </div>
          </div>
        </section>

        <aside
          class="w-full md:w-[340px] bg-surface-container border-t md:border-t-0 md:border-l border-outline-variant/40 flex flex-col md:h-full shrink-0 overflow-y-auto scroll-thin max-h-[40vh] md:max-h-none"
        >
          <div v-if="config" class="py-unit px-md md:p-margin space-y-unit md:space-y-xl">
            <Recorder
              v-if="listing?.path"
              ref="recorderRef"
              :workdir="listing.path"
              :headless="true"
              @saved="onRecordingSaved"
            />
            <details
              :open="configOpen"
              @toggle="(e: Event) => (configOpen = (e.target as HTMLDetailsElement).open)"
            >
              <summary
                class="flex items-center justify-between cursor-pointer list-none mb-unit md:mb-md md:pointer-events-none gap-xs"
              >
                <h3 class="text-titleSmall text-on-surface flex items-center gap-unit">
                  <span class="material-symbols-outlined text-[18px] md:hidden">tune</span>
                  Configuration
                </h3>
                <button
                  v-if="recorderRef && !recorderRef.recording"
                  @click.stop.prevent="recorderRef?.start()"
                  class="min-h-9 px-md inline-flex items-center gap-unit bg-error-container text-on-error-container rounded-full font-titleSmall hover:opacity-90 transition-opacity"
                  title="Record"
                >
                  <span
                    class="material-symbols-outlined text-[16px]"
                    style="font-variation-settings: &quot;FILL&quot; 1"
                    >fiber_manual_record</span
                  >
                  Rec
                </button>
                <button
                  v-else-if="recorderRef"
                  @click.stop.prevent="recorderRef?.stop()"
                  class="min-h-9 px-md inline-flex items-center gap-unit bg-primary text-on-primary rounded-full font-titleSmall font-bold hover:opacity-90 transition-opacity"
                  :title="`Stop recording \u00b7 ${recorderRef?.elapsed}`"
                >
                  <span
                    class="material-symbols-outlined text-[16px]"
                    style="font-variation-settings: &quot;FILL&quot; 1"
                    >stop</span
                  >
                  {{ recorderRef?.elapsed }}
                </button>
                <span
                  v-else
                  class="font-mono text-labelSmall flex items-center gap-unit"
                  :class="
                    saveState === 'saving'
                      ? 'text-secondary'
                      : saveState === 'saved'
                        ? 'text-tertiary'
                        : 'text-outline'
                  "
                >
                  <span
                    class="w-1.5 h-1.5 rounded-full"
                    :class="
                      saveState === 'saving'
                        ? 'bg-secondary animate-pulse'
                        : saveState === 'saved'
                          ? 'bg-tertiary'
                          : 'bg-outline-variant'
                    "
                  ></span>
                  {{
                    saveState === "saving" ? "saving" : saveState === "saved" ? "saved" : "synced"
                  }}
                </span>
              </summary>

              <div class="space-y-md">
                <label class="block">
                  <span
                    class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide"
                    >Model</span
                  >
                  <select
                    v-model="config.model"
                    :class="[fieldClass, 'mt-unit']"
                    @change="onModelChanged"
                  >
                    <option v-for="m in allAsrModels" :key="m.id" :value="m.id">
                      {{ m.display_name
                      }}{{ m.status === "installed" ? "" : " \u2014 not installed" }}
                    </option>
                  </select>
                </label>

                <div
                  v-if="selectedAsrModel && !selectedModelInstalled"
                  class="flex items-center gap-md p-md rounded-lg bg-error-container/40 border border-error/40"
                >
                  <span class="material-symbols-outlined text-error">cloud_download</span>
                  <div class="flex-1 min-w-0">
                    <div class="text-bodyMedium text-on-surface">Model not installed</div>
                    <div class="text-labelSmall text-on-surface-variant truncate">
                      {{ selectedAsrModel.display_name }} ·
                      {{ (selectedAsrModel.size_bytes / 1048576).toFixed(0) }} MB
                    </div>
                  </div>
                  <button
                    type="button"
                    class="px-md h-9 rounded-md bg-primary text-on-primary text-labelLarge inline-flex items-center gap-xs disabled:opacity-50"
                    :disabled="installingSelected || selectedAsrModel.status === 'downloading'"
                    @click="installSelectedModel"
                  >
                    <Spinner
                      v-if="installingSelected || selectedAsrModel.status === 'downloading'"
                      :size="16"
                    />
                    <span v-else class="material-symbols-outlined text-[18px]">download</span>
                    {{
                      selectedAsrModel.status === "downloading"
                        ? "Downloading…"
                        : installingSelected
                          ? "Starting…"
                          : "Download"
                    }}
                  </button>
                </div>

                <div class="grid grid-cols-2 gap-md">
                  <label class="block">
                    <span
                      class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide"
                      >Language</span
                    >
                    <select v-model="config.language" :class="[fieldClass, 'mt-unit']">
                      <option v-for="l in languageOptions" :key="l" :value="l">
                        {{ l === "auto" ? "Auto" : l }}
                      </option>
                    </select>
                  </label>
                  <label class="block">
                    <span
                      class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide"
                      >Device</span
                    >
                    <select v-model="config.device" :class="[fieldClass, 'mt-unit']">
                      <option value="cpu">CPU</option>
                      <option v-if="sys?.cuda_available" value="cuda">CUDA</option>
                    </select>
                  </label>
                </div>

                <div class="grid grid-cols-2 gap-md">
                  <label class="block">
                    <span
                      class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide"
                      >Diarizer</span
                    >
                    <select
                      v-model="config.diarizer"
                      :disabled="!config.diarize"
                      :class="[fieldClass, 'mt-unit', !config.diarize ? 'opacity-50' : '']"
                    >
                      <option value="auto">Auto</option>
                      <option v-if="!sys?.is_mobile" value="nemo">NVIDIA NeMo Sortformer</option>
                      <option value="eres2net">pyannote-3.0 + ERes2Net-base</option>
                      <option value="titanet">pyannote-3.0 + TitaNet-Large</option>
                    </select>
                  </label>
                  <label class="block">
                    <span
                      class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide"
                      >Speakers</span
                    >
                    <select
                      :value="config.speakers ?? 0"
                      :class="[fieldClass, 'mt-unit']"
                      @change="
                        (e) => {
                          const n = Number((e.target as HTMLSelectElement).value);
                          if (!config) return;
                          config.speakers = n > 0 ? n : null;
                          if (n > 0 && !config.diarize) config.diarize = true;
                        }
                      "
                    >
                      <option v-for="o in speakerOptions" :key="o.value" :value="o.value">
                        {{ o.label }}
                      </option>
                    </select>
                  </label>
                </div>

                <div class="flex items-center justify-between gap-xl py-xs">
                  <div class="flex items-center justify-between gap-xs flex-1 min-w-0">
                    <div class="text-bodyMedium text-on-surface truncate">Auto-Diarize</div>
                    <button
                      type="button"
                      class="w-10 h-6 rounded-full relative shrink-0 transition-colors"
                      :class="
                        config.diarize
                          ? 'bg-primary'
                          : 'bg-surface-container-highest border border-outline-variant'
                      "
                      @click="config.diarize = !config.diarize"
                    >
                      <span
                        class="absolute top-1 w-4 h-4 rounded-full transition-all"
                        :class="config.diarize ? 'right-1 bg-on-primary' : 'left-1 bg-outline'"
                      ></span>
                    </button>
                  </div>

                  <div class="flex items-center justify-between gap-xs flex-1 min-w-0">
                    <div class="text-bodyMedium text-on-surface truncate">Auto-Rename</div>
                    <button
                      type="button"
                      class="w-10 h-6 rounded-full relative shrink-0 transition-colors"
                      :class="
                        config.auto_rename
                          ? 'bg-primary'
                          : 'bg-surface-container-highest border border-outline-variant'
                      "
                      @click="config.auto_rename = !config.auto_rename"
                    >
                      <span
                        class="absolute top-1 w-4 h-4 rounded-full transition-all"
                        :class="config.auto_rename ? 'right-1 bg-on-primary' : 'left-1 bg-outline'"
                      ></span>
                    </button>
                  </div>
                </div>
              </div>
            </details>

            <div class="hidden md:block">
              <h3 class="text-titleSmall text-on-surface mb-md">Selection</h3>
              <div
                class="bg-surface-container-high p-md rounded-lg space-y-xs font-mono text-labelMedium"
              >
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">File</span>
                  <span
                    class="text-on-surface truncate ml-md max-w-[180px]"
                    :title="selectedEntry?.name"
                  >
                    {{ selectedEntry?.name ?? "—" }}
                  </span>
                </div>
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Status</span>
                  <span
                    :class="
                      status === 'error'
                        ? 'text-error'
                        : status === 'idle'
                          ? 'text-tertiary'
                          : 'text-secondary'
                    "
                  >
                    <template
                      v-if="
                        selectedEntry && progressByPath[selectedEntry.path] && status === 'running'
                      "
                    >
                      {{ phaseLabel(progressByPath[selectedEntry.path].phase) }}
                      <span v-if="progressByPath[selectedEntry.path].phase === 'transcribing'">
                        · {{ progressByPath[selectedEntry.path].displayPct.toFixed(1) }}%
                      </span>
                    </template>
                    <template v-else>{{
                      status === "idle" && transcript ? "ready" : status
                    }}</template>
                  </span>
                </div>
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Duration</span>
                  <span class="text-on-surface">{{
                    transcript ? fmtLong(transcript.duration_ms) : "—"
                  }}</span>
                </div>
                <div v-if="transcript" class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Utterances · Speakers</span>
                  <span class="text-on-surface">
                    {{ transcript.utterances.length }} ·
                    <span class="text-primary">{{ transcript.speakers_detected }}</span>
                  </span>
                </div>
              </div>
            </div>
          </div>
        </aside>
      </template>

      <Settings v-else-if="tab === 'settings'" />
      <LogViewer v-else-if="tab === 'logs'" />
    </main>

    <div
      v-if="
        tab === 'transcribe' &&
        (recorderRef?.recording ||
          (selectedEntry && progressByPath[selectedEntry.path] && status === 'running') ||
          transcript)
      "
      class="md:hidden shrink-0 border-t border-outline-variant/40 bg-surface-container-low px-margin py-xs flex items-center gap-xs font-mono text-labelSmall overflow-hidden"
    >
      <template v-if="recorderRef?.recording">
        <span class="w-1.5 h-1.5 rounded-full bg-error animate-pulse shrink-0"></span>
        <span class="text-error uppercase tracking-wide">REC</span>
        <span class="text-on-surface ml-auto">{{ recorderRef?.elapsed }}</span>
      </template>
      <template
        v-else-if="selectedEntry && progressByPath[selectedEntry.path] && status === 'running'"
      >
        <span class="w-1.5 h-1.5 rounded-full bg-secondary animate-pulse shrink-0"></span>
        <span class="text-on-surface truncate min-w-0 flex-1" :title="selectedEntry.name">{{
          selectedEntry.name
        }}</span>
        <span class="text-secondary shrink-0">
          <template v-if="progressByPath[selectedEntry.path].phase === 'transcribing'">
            {{ progressByPath[selectedEntry.path].displayPct.toFixed(0) }}% ·
            {{ fmtClock(progressByPath[selectedEntry.path].elapsedSec) }} / ETA
            {{ fmtClock(progressByPath[selectedEntry.path].etaSec) }}
          </template>
          <template v-else>
            {{ phaseLabel(progressByPath[selectedEntry.path].phase) }} ·
            {{ fmtClock(progressByPath[selectedEntry.path].elapsedSec) }}
          </template>
        </span>
      </template>
      <template v-else-if="transcript">
        <span class="material-symbols-outlined text-[14px] text-tertiary shrink-0"
          >check_circle</span
        >
        <span class="text-on-surface truncate min-w-0 flex-1" :title="selectedEntry?.name">{{
          selectedEntry?.name ?? "—"
        }}</span>
        <span class="text-on-surface-variant shrink-0"
          >{{ fmtLong(transcript.duration_ms) }} · {{ transcript.utterances.length }} utt ·
          {{ transcript.speakers_detected }} spk</span
        >
      </template>
    </div>

    <nav
      class="md:hidden flex items-stretch shrink-0 border-t border-outline-variant/40 bg-surface"
    >
      <button
        v-for="t in tabs"
        :key="t.id"
        @click="tab = t.id"
        class="flex-1 flex flex-col items-center justify-center h-14 transition-colors"
        :class="tab === t.id ? 'text-primary' : 'text-on-surface-variant hover:text-on-surface'"
        :title="t.label"
        :aria-label="t.label"
      >
        <TranscribeIcon v-if="t.id === 'transcribe'" :size="26" />
        <SettingsIcon v-else-if="t.id === 'settings'" :size="26" />
        <LogIcon v-else :size="26" />
      </button>
    </nav>

    <div
      v-if="renaming"
      class="fixed inset-0 z-40 bg-black/50 flex items-center justify-center"
      @click.self="renaming = false"
    >
      <div
        class="bg-surface-container rounded-xl border border-outline-variant/40 p-margin w-[420px] max-w-[90vw] space-y-md"
      >
        <h3 class="text-titleSmall text-on-surface">Rename file</h3>
        <input
          v-model="renameValue"
          :class="fieldClass"
          @keydown.enter="commitRename"
          @keydown.escape="renaming = false"
        />
        <div class="flex justify-end gap-xs">
          <button
            class="px-md py-xs rounded-full border border-outline-variant text-on-surface text-titleSmall hover:bg-surface-container-high"
            @click="renaming = false"
          >
            Cancel
          </button>
          <button
            class="px-md py-xs rounded-full bg-primary text-on-primary text-titleSmall hover:bg-primary-fixed-dim"
            @click="commitRename"
          >
            Rename
          </button>
        </div>
      </div>
    </div>

    <div
      v-if="exporting"
      class="fixed inset-0 z-40 bg-black/50 flex items-center justify-center"
      @click.self="exporting = false"
    >
      <div
        class="bg-surface-container rounded-xl border border-outline-variant/40 p-margin w-[420px] max-w-[90vw] space-y-md"
      >
        <h3 class="text-titleSmall text-on-surface">Export transcript</h3>
        <div class="space-y-xs">
          <label
            v-for="f in exportFormats"
            :key="f.value"
            class="flex items-center gap-xs p-xs rounded hover:bg-surface-container-high cursor-pointer"
          >
            <input type="radio" :value="f.value" v-model="exportFormat" />
            <span class="text-bodyMedium text-on-surface">{{ f.label }}</span>
          </label>
        </div>
        <div class="flex justify-end gap-xs">
          <button
            class="px-md py-xs rounded-full border border-outline-variant text-on-surface text-titleSmall hover:bg-surface-container-high"
            @click="exporting = false"
          >
            Cancel
          </button>
          <button
            class="px-md py-xs rounded-full bg-primary text-on-primary text-titleSmall hover:bg-primary-fixed-dim"
            @click="commitExport"
          >
            Save…
          </button>
        </div>
      </div>
    </div>

    <div
      v-if="trimming"
      class="fixed inset-0 z-40 bg-black/60 flex items-center justify-center p-margin"
      @click.self="closeTrim"
      @keydown.escape="closeTrim"
    >
      <div
        class="bg-surface-container rounded-xl border border-outline-variant/50 w-full max-w-[768px] flex flex-col overflow-hidden shadow-2xl"
      >
        <div
          class="px-margin py-md border-b border-outline-variant/40 bg-surface-container-low flex items-start gap-md"
        >
          <span class="material-symbols-outlined text-primary text-[22px] mt-unit"
            >content_cut</span
          >
          <div class="flex-1 min-w-0">
            <h3 class="text-titleSmall text-on-surface">Select range to transcribe</h3>
            <p
              class="font-mono text-labelSmall text-on-surface-variant truncate"
              :title="trimTarget?.name"
            >
              {{ trimTarget?.name ?? "—" }}
            </p>
          </div>
          <button
            class="material-symbols-outlined text-[20px] p-xs rounded hover:bg-surface-container-high text-on-surface-variant hover:text-on-surface transition-colors"
            title="Close"
            @click="closeTrim"
          >
            close
          </button>
        </div>

        <div
          v-if="trimError"
          class="mx-margin mt-md p-md rounded-lg bg-error-container/30 border border-error/40 text-error text-bodyMedium font-mono"
        >
          {{ trimError }}
        </div>

        <div class="px-margin py-md space-y-md">
          <div
            class="rounded-lg border border-outline-variant/50 bg-surface-container-low p-md relative"
          >
            <div
              v-if="trimLoading"
              class="absolute inset-0 flex items-center justify-center text-on-surface-variant gap-xs"
            >
              <span class="material-symbols-outlined text-[20px] animate-pulse">graphic_eq</span>
              <span class="font-mono text-labelSmall">analysing…</span>
            </div>
            <div ref="waveformBox" class="relative select-none touch-none">
              <canvas
                ref="trimCanvas"
                width="720"
                height="120"
                class="w-full h-28 block pointer-events-none"
              ></canvas>
              <div
                class="absolute top-0 bottom-0 bg-primary/15 border-l-2 border-r-2 border-primary pointer-events-none"
                :style="{
                  left: trimStartFrac * 100 + '%',
                  right: (1 - trimEndFrac) * 100 + '%',
                }"
              ></div>
              <div
                class="absolute -top-2 -bottom-2 -ml-7 w-14 cursor-ew-resize flex items-center justify-center touch-none"
                :style="{ left: trimStartFrac * 100 + '%' }"
                @pointerdown="(e) => beginHandleDrag('start', e)"
              >
                <div class="w-2 h-full bg-primary rounded-full shadow-lg"></div>
              </div>
              <div
                class="absolute -top-2 -bottom-2 -ml-7 w-14 cursor-ew-resize flex items-center justify-center touch-none"
                :style="{ left: trimEndFrac * 100 + '%' }"
                @pointerdown="(e) => beginHandleDrag('end', e)"
              >
                <div class="w-2 h-full bg-primary rounded-full shadow-lg"></div>
              </div>
              <div
                v-if="trimDuration > 0"
                class="absolute top-0 bottom-0 w-px bg-tertiary pointer-events-none shadow-[0_0_4px_rgba(255,255,255,0.6)]"
                :style="{ left: trimPlayheadFrac * 100 + '%' }"
              >
                <div
                  class="absolute -top-1 -translate-x-1/2 left-0 w-2 h-2 rounded-full bg-tertiary"
                ></div>
              </div>
            </div>
          </div>

          <div
            class="flex justify-between font-mono text-labelMedium text-on-surface-variant pt-xs border-t border-outline-variant/40"
          >
            <span class="text-primary">{{ fmt(trimStart) }}</span>
            <span class="text-on-surface"
              >selected {{ fmt(Math.max(0, trimEnd - trimStart)) }}</span
            >
            <span class="text-primary">{{ fmt(trimEnd) }}</span>
          </div>
        </div>

        <div
          class="px-margin py-md border-t border-outline-variant/40 bg-surface-container-low flex justify-between items-center gap-xs"
        >
          <button
            class="min-h-12 px-lg rounded-full border border-outline-variant text-on-surface text-titleSmall hover:bg-surface-container-high transition-colors flex items-center gap-unit"
            @click="resetTrim"
            title="Reset to full track"
          >
            <span class="material-symbols-outlined text-[18px]">restart_alt</span>
            Full track
          </button>
          <div class="flex gap-xs">
            <button
              class="min-h-12 w-12 rounded-full border border-outline-variant text-primary hover:bg-surface-container-high transition-colors flex items-center justify-center"
              :title="trimPlaying ? 'Stop' : 'Play selection'"
              :disabled="trimAudioLoading || !trimTarget"
              @click="toggleTrimPlay"
            >
              <Spinner v-if="trimAudioLoading" :size="20" />
              <span
                v-else
                class="material-symbols-outlined text-[22px]"
                style="font-variation-settings: &quot;FILL&quot; 1"
                >{{ trimPlaying ? "pause" : "play_arrow" }}</span
              >
            </button>
            <button
              class="min-h-12 w-12 rounded-full border border-outline-variant text-on-surface hover:bg-surface-container-high transition-colors flex items-center justify-center"
              title="Cancel"
              @click="closeTrim"
            >
              <CancelIcon :size="20" />
            </button>
            <button
              class="min-h-12 w-12 rounded-full bg-primary text-on-primary hover:bg-primary-fixed-dim transition-colors flex items-center justify-center"
              title="Save"
              @click="commitTrim"
            >
              <SaveIcon :size="20" />
            </button>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
