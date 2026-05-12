<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { open, save, confirm, message } from "@tauri-apps/plugin-dialog";
import { readFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, events } from "@/api";
import type {
  Config,
  DirEntry,
  DirListing,
  ExportFormat,
  ModelInfo,
  SystemInfo,
  TranscribeProgress,
  Transcript,
} from "@/types";
import Settings from "@components/Settings.vue";
import LogViewer from "@components/LogViewer.vue";
import SetupGate from "@components/SetupGate.vue";
import StorageGate from "@components/StorageGate.vue";
import BottomNav from "@components/BottomNav.vue";
import type { Tab } from "@components/nav-tabs";
import AppHeader from "@components/AppHeader.vue";
import StatusStrip from "@components/StatusStrip.vue";
import FileList from "@components/FileList.vue";
import TranscriptPanel from "@components/TranscriptPanel.vue";
import ConfigPanel from "@components/ConfigPanel.vue";
import Recorder from "@components/Recorder.vue";
import ErrorBanner from "@components/ui/ErrorBanner.vue";
import Button from "@components/ui/Button.vue";
import RenameDialog from "@components/dialogs/RenameDialog.vue";
import ExportDialog from "@components/dialogs/ExportDialog.vue";
import RedoDiarizeDialog from "@components/dialogs/RedoDiarizeDialog.vue";
import TrimDialog from "@components/dialogs/TrimDialog.vue";
import { audioExtensions, basenameOf, hasAudioExt } from "@utils/audio";
import { applyMissingModelDefaults, applySystemConfigDefaults } from "@utils/models";
import { useDebouncedSave } from "@composables/useDebouncedSave";
import { useEssentials } from "@composables/useEssentials";
import { useFileSelection } from "@composables/useFileSelection";
import { recordOmit, recordSet } from "@utils/records";

const tab = ref<Tab>("transcribe");
const logRetain = ref<number>(Number(localStorage.getItem("wt.logRetain") ?? "1") || 1);
const logAuto = ref(true);
const logViewerRef = ref<InstanceType<typeof LogViewer> | null>(null);
watch(logRetain, (v) => localStorage.setItem("wt.logRetain", String(v)));
const sys = ref<SystemInfo | null>(null);
const config = ref<Config | null>(null);
const models = ref<ModelInfo[]>([]);
const listing = ref<DirListing | null>(null);
const selectedPath = ref<string>("");
const selection = useFileSelection();
const selectedPaths = selection.selected;
const transcript = ref<Transcript | null>(null);
const status = ref<"idle" | "running" | "renaming" | "error">("idle");
const error = ref<string | null>(null);
const dragOver = ref(false);
const busy = ref<Record<string, boolean>>({});
const progressByPath = ref<Record<string, TranscribeProgress>>({});
const cancelledPaths = ref<Set<string>>(new Set());

const essentials = useEssentials(models);
const essentialIds = essentials.ids;
const essentialProgress = essentials.progress;
const essentialErrors = essentials.errors;
const essentialRuntimes = essentials.runtimes;
const essentialsReady = essentials.ready;

const storageGateResolved = ref(false);
const showStorageGate = ref(false);

const dialogOpen = ref(false);
const queueActive = ref(false);
const queueTotal = ref(0);
const queueDone = ref(0);

const recorderRef = ref<InstanceType<typeof Recorder> | null>(null);
const fileListRef = ref<InstanceType<typeof FileList> | null>(null);

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
const transcribedCount = computed(() => audioEntries.value.filter((e) => !!e.cache_key).length);

const selectedAsrModel = computed(
  () => models.value.find((m) => m.family === "asr" && m.id === config.value?.model) ?? null,
);
const selectedModelInstalled = computed(() => selectedAsrModel.value?.status === "installed");

async function withDialog<T>(fn: () => Promise<T>): Promise<T | undefined> {
  if (dialogOpen.value) return undefined;
  dialogOpen.value = true;
  try {
    return await fn();
  } finally {
    dialogOpen.value = false;
  }
}

async function reload() {
  config.value = await api.loadConfig();
  models.value = await api.listModels();
  if (!listing.value) {
    const start = config.value.last_dir || (await api.defaultDir());
    await openDir(start);
  } else {
    await refreshListing();
  }
}

function probeMissingDurations() {
  if (!listing.value) return;
  for (const entry of listing.value.entries) {
    if (!entry.is_audio || entry.duration_ms != null) continue;
    const target = entry;
    void api
      .probeDuration(target.path)
      .then((ms) => {
        if (ms != null && target.duration_ms == null) target.duration_ms = ms;
      })
      .catch(() => {});
  }
}

function mergePrevDurations(prev: DirEntry[], next: DirEntry[]) {
  const byPath = new Map(prev.map((e) => [e.path, e.duration_ms]));
  for (const e of next) {
    if (e.duration_ms == null) {
      const seen = byPath.get(e.path);
      if (seen != null) e.duration_ms = seen;
    }
  }
}

async function refreshListing() {
  if (!listing.value) return;
  const prev = listing.value.entries;
  try {
    const next = await api.listDirectory(listing.value.path);
    mergePrevDurations(prev, next.entries);
    listing.value = next;
    probeMissingDurations();
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
    probeMissingDurations();
  } catch (e) {
    error.value = String(e);
  }
}

async function pickAudio() {
  const selected = await withDialog(() =>
    open({ multiple: true, filters: [{ name: "Audio", extensions: [...audioExtensions] }] }),
  );
  if (!selected) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  await addPathsToWorkdir(paths);
}

const yieldToUI = () => new Promise<void>((r) => setTimeout(r, 0));

function addPathsToWorkdir(paths: string[]) {
  if (!listing.value) return;
  const dir = listing.value.path;
  const entries = listing.value.entries;

  const audioPaths = paths.filter(hasAudioExt).filter((p) => !entries.some((e) => e.path === p));
  if (audioPaths.length > 200) {
    audioPaths.splice(200);
    error.value = "only the first 200 files were queued";
  }
  if (audioPaths.length === 0) return;

  type Stub = (typeof entries)[number];
  const stubs: Stub[] = audioPaths.map((p) => {
    const stub: Stub = {
      name: basenameOf(p),
      path: p,
      is_dir: false,
      is_audio: true,
      size_bytes: 0,
      modified_ms: 0,
      cache_key: null,
      utterances: null,
      duration_ms: null,
      trim_start_ms: null,
      trim_end_ms: null,
    };
    entries.push(stub);
    return stub;
  });
  selectedPath.value = stubs[stubs.length - 1].path;

  for (const stub of stubs) {
    void api
      .probeDuration(stub.path)
      .then((ms) => {
        if (ms != null && stub.duration_ms == null) stub.duration_ms = ms;
      })
      .catch(() => {});
  }

  const copyOne = async (stub: Stub): Promise<string> => {
    const source = stub.path;
    let eRaw: unknown;
    try {
      return await api.addToWorkdir(source, dir);
    } catch (e) {
      eRaw = e;
    }
    if (!sys.value?.is_mobile) {
      throw eRaw instanceof Error ? eRaw : new Error(String(eRaw));
    }
    try {
      const bytes = await readFile(source);
      if (bytes.byteLength > 200 * 1024 * 1024) {
        throw new Error("file exceeds 200 MB limit for in-process copy");
      }
      await yieldToUI();
      return await api.saveRecording(dir, basenameOf(source), bytes);
    } catch (e2) {
      throw new Error(`${eRaw} / ${e2}`);
    }
  };

  void (async () => {
    let lastDest: string | null = null;
    for (let i = 0; i < stubs.length; i += 3) {
      const batch = stubs.slice(i, i + 3);
      await yieldToUI();
      await Promise.allSettled(
        batch.map(async (stub) => {
          const source = stub.path;
          try {
            const destPath = await copyOne(stub);
            stub.name = basenameOf(destPath);
            stub.path = destPath;
            lastDest = destPath;
            if (stub.duration_ms == null) {
              void api
                .probeDuration(destPath)
                .then((ms) => {
                  if (ms != null && stub.duration_ms == null) stub.duration_ms = ms;
                })
                .catch(() => {});
            }
          } catch (e) {
            error.value = String(e);
            const idx = entries.findIndex((en) => en.path === source);
            if (idx !== -1) entries.splice(idx, 1);
          }
        }),
      );
    }
    if (lastDest !== null && selectedPath.value === stubs[stubs.length - 1].path) {
      selectedPath.value = lastDest;
    }
    await refreshListing();
  })();
}

async function onRecordingSaved(path: string) {
  await refreshListing();
  selectedPath.value = path;
  transcript.value = null;
  error.value = null;
}

async function onRenameSpeaker(payload: { old: string; name: string }) {
  const key = selectedEntry.value?.cache_key;
  if (!key || !transcript.value) return;
  try {
    transcript.value = await api.renameSpeaker(key, payload.old, payload.name);
  } catch (e) {
    error.value = `rename speaker failed: ${String(e)}`;
  }
}

function closeTranscript() {
  transcript.value = null;
  selectedPath.value = "";
  error.value = null;
}

function chooseEntry(entry: DirEntry) {
  if (entry.path === selectedPath.value) return;
  selectedPath.value = entry.path;
  transcript.value = null;
  error.value = null;
}

function viewEntry(entry: DirEntry) {
  if (entry.path !== selectedPath.value) {
    selectedPath.value = entry.path;
    transcript.value = null;
    error.value = null;
  }
  if (entry.cache_key) void loadCached(entry.cache_key);
}

async function loadCached(key: string) {
  try {
    const t = await api.historyLoad(key);
    if (t) transcript.value = t;
  } catch (e) {
    error.value = String(e);
  }
}

const unlisten: (() => void)[] = [];

onMounted(async () => {
  await essentials.init();
  const refreshModels = async (id?: string) => {
    models.value = await api.listModels();
    if (id && error.value && error.value.includes("not installed") && config.value?.model === id) {
      error.value = null;
    }
  };
  unlisten.push(
    await events.onTranscribeProgress((p) => {
      if (cancelledPaths.value.has(p.path)) return;
      recordSet(progressByPath, p.path, p);
    }),
    ...(await essentials.attachListeners(refreshModels)),
    await getCurrentWebview().onDragDropEvent((event) => {
      if (tab.value !== "transcribe") return;
      if (event.payload.type === "over") dragOver.value = true;
      else if (event.payload.type === "leave") dragOver.value = false;
      else if (event.payload.type === "drop") {
        dragOver.value = false;
        const paths = event.payload.paths ?? [];
        if (paths.length) void addPathsToWorkdir(paths);
      }
    }),
  );
  function onKeyDown(e: KeyboardEvent) {
    if (tab.value !== "transcribe") return;
    if (dialogOpen.value) return;
    const ctrl = e.ctrlKey || e.metaKey;
    if (ctrl && (e.key === "a" || e.key === "A")) {
      e.preventDefault();
      selection.selectAll(audioPathsList);
    } else if (e.key === "Escape" && selectedPaths.value.size > 0) {
      clearSelection();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selectedPaths.value.size > 0) {
      e.preventDefault();
      void bulkDelete();
    }
  }
  document.addEventListener("keydown", onKeyDown);
  unlisten.push(() => document.removeEventListener("keydown", onKeyDown));

  sys.value = await api.systemInfo();
  await reload();
  if (config.value) {
    applySystemConfigDefaults(config.value, sys.value);
    applyMissingModelDefaults(config.value, models.value);
  }

  if (sys.value?.os === "android") {
    try {
      const granted = await api.hasPersistentStorage();
      if (!granted && sessionStorage.getItem("wt:storageGateSkipped") !== "1") {
        showStorageGate.value = true;
      } else {
        if (granted) {
          await api.enablePersistentStorage();
          models.value = await api.listModels();
        }
        storageGateResolved.value = true;
      }
    } catch {
      storageGateResolved.value = true;
    }
  } else {
    storageGateResolved.value = true;
  }

  if (storageGateResolved.value) {
    void startEssentialsSafely();
  }
});

async function startEssentialsSafely() {
  try {
    await essentials.start();
  } catch (e) {
    error.value = `essentials start failed: ${String(e)}`;
  }
}

async function onStorageGranted() {
  showStorageGate.value = false;
  storageGateResolved.value = true;
  try {
    models.value = await api.listModels();
  } catch (e) {
    error.value = String(e);
  }
  void startEssentialsSafely();
}

function onStorageSkipped() {
  sessionStorage.setItem("wt:storageGateSkipped", "1");
  showStorageGate.value = false;
  storageGateResolved.value = true;
  void startEssentialsSafely();
}

onUnmounted(() => {
  unlisten.forEach((u) => u());
});

watch(tab, (t) => {
  if (t === "transcribe") void refreshListing();
});

const { error: saveError } = useDebouncedSave(config, (next) => api.saveConfig(next));
watch(saveError, (e) => {
  if (e) error.value = `save failed: ${e}`;
});

async function runTranscribe(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || !config.value || !target.is_audio) return;
  cancelledPaths.value.delete(target.path);
  if (!selectedModelInstalled.value) {
    error.value = `Model "${selectedAsrModel.value?.display_name ?? config.value.model}" is not installed. Download it in Configuration.`;
    tab.value = "transcribe";
    return;
  }
  selectedPath.value = target.path;
  status.value = "running";
  error.value = null;
  transcript.value = null;
  recordSet(busy, target.path, true);
  try {
    transcript.value = await api.transcribeFile(target.path, config.value);
    status.value = "idle";
    await refreshListing();
    if (config.value.auto_rename) {
      const renamed = audioEntries.value.find((e) => e.path === target.path) ?? target;
      await autoRename(renamed, { silent: true });
    }
  } catch (e) {
    const msg = String(e);
    if (msg.includes("cancelled")) {
      status.value = "idle";
    } else {
      error.value = msg;
      status.value = "error";
    }
  } finally {
    recordOmit(busy, target.path);
    recordOmit(progressByPath, target.path);
  }
}

async function stopTranscribe(entry: DirEntry) {
  cancelledPaths.value.add(entry.path);
  for (const p of Object.keys(busy.value)) cancelledPaths.value.add(p);
  for (const p of Object.keys(progressByPath.value)) cancelledPaths.value.add(p);
  progressByPath.value = {};
  busy.value = {};
  status.value = "idle";
  queueActive.value = false;
  queueTotal.value = 0;
  queueDone.value = 0;
  await api.cancelAllTranscribes();
}

async function transcribeAll(targets?: DirEntry[]) {
  if (!config.value || queueActive.value) return;
  const items = targets ?? untranscribedEntries.value;
  if (!items.length) return;
  queueActive.value = true;
  queueTotal.value = items.length;
  queueDone.value = 0;
  try {
    for (const entry of items) {
      if (!queueActive.value) break;
      await runTranscribe(entry);
      if (!queueActive.value) break;
      queueDone.value += 1;
    }
  } finally {
    queueActive.value = false;
    queueTotal.value = 0;
    queueDone.value = 0;
  }
}

const audioPathsList = () => audioEntries.value.map((e) => e.path);
function toggleSelect(path: string) {
  selection.toggle(path);
}
function rangeSelect(anchor: string) {
  selection.range(anchor, audioPathsList);
}
function clearSelection() {
  selection.clear();
}

async function bulkDelete() {
  const targets = [...selectedPaths.value];
  if (!targets.length) return;
  clearSelection();
  for (const path of targets) {
    try {
      await api.deleteFile(path);
      if (selectedPath.value === path) {
        selectedPath.value = "";
        transcript.value = null;
      }
    } catch (e) {
      error.value = String(e);
    }
  }
  await refreshListing();
}

async function bulkTranscribe() {
  const targets = [...selectedPaths.value]
    .map((p) => audioEntries.value.find((e) => e.path === p))
    .filter((e): e is DirEntry => !!e && !busy.value[e.path]);
  if (!targets.length) return;
  await transcribeAll(targets);
}

const autoRenamingPath = ref<string | null>(null);
async function autoRename(entry?: DirEntry, opts?: { silent?: boolean }) {
  const target = entry ?? selectedEntry.value;
  if (!target || !target.is_audio || autoRenamingPath.value) return;
  autoRenamingPath.value = target.path;
  try {
    let t = transcript.value;
    if (!t && target.cache_key) t = await api.historyLoad(target.cache_key);
    if (!t) {
      if (!opts?.silent) {
        await message("Transcribe first to enable auto-rename.", {
          title: "Auto-rename",
          kind: "info",
        });
      }
      return;
    }
    status.value = "renaming";
    try {
      const s = await api.suggestFilename(t);
      const ext = target.name.includes(".") ? target.name.split(".").pop() : "";
      const suggestion = `${s.topic}_${s.stamp}${ext ? "." + ext : ""}`;
      if (!opts?.silent) {
        const ok = await withDialog(() =>
          confirm(`Rename to:\n\n${suggestion}`, { title: "Auto-rename", okLabel: "Rename" }),
        );
        if (!ok) return;
      }
      const newPath = await api.renameFile(target.path, suggestion);
      selectedPath.value = newPath;
      await refreshListing();
    } catch (e) {
      error.value = `auto-rename failed: ${String(e)}`;
    } finally {
      status.value = "idle";
    }
  } finally {
    autoRenamingPath.value = null;
  }
}

const renaming = ref(false);
const renameTarget = ref<DirEntry | null>(null);
const renameValue = ref("");

function openRename(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || renaming.value || exporting.value) return;
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
    confirm(
      `Remove "${target.name}" from this folder?\n\nThe original file in its source location is not affected.`,
      {
        title: "Delete file",
        okLabel: "Delete",
        kind: "warning",
      },
    ),
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

const redoDiarizeOpen = ref(false);
const redoDiarizeTarget = ref<DirEntry | null>(null);
const redoDiarizeDiarizer = ref<Config["diarizer"]>("sortformer-onnx");
const redoDiarizeSpeakers = ref<number>(0);

function openRedoDiarize(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || !target.cache_key || busy.value[target.path]) return;
  redoDiarizeTarget.value = target;
  redoDiarizeDiarizer.value = config.value?.diarizer ?? "sortformer-onnx";
  redoDiarizeSpeakers.value = config.value?.speakers ?? 0;
  redoDiarizeOpen.value = true;
}

async function commitRedoDiarize() {
  const target = redoDiarizeTarget.value;
  if (!target || !target.cache_key || !config.value) return;
  redoDiarizeOpen.value = false;
  const overrideConfig: Config = {
    ...config.value,
    diarize: true,
    diarizer: redoDiarizeDiarizer.value,
    speakers: redoDiarizeSpeakers.value > 0 ? redoDiarizeSpeakers.value : null,
  };
  selectedPath.value = target.path;
  status.value = "running";
  error.value = null;
  recordSet(busy, target.path, true);
  try {
    transcript.value = await api.redoDiarization(target.path, target.cache_key, overrideConfig);
    status.value = "idle";
    await refreshListing();
    if (config.value.auto_rename) {
      const renamed = audioEntries.value.find((e) => e.path === target.path) ?? target;
      await autoRename(renamed, { silent: true });
    }
  } catch (e) {
    error.value = `Re-diarize failed: ${String(e)}`;
    status.value = "error";
  } finally {
    recordOmit(busy, target.path);
    redoDiarizeTarget.value = null;
  }
}

const trimTarget = ref<DirEntry | null>(null);
function openTrim(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || !target.is_audio) return;
  if (trimTarget.value || renaming.value || exporting.value) return;
  trimTarget.value = target;
}
async function onTrimSaved(payload?: {
  path: string;
  durationMs: number | null;
  trimmed: boolean;
}) {
  trimTarget.value = null;
  if (payload?.trimmed && listing.value) {
    for (const entry of listing.value.entries) {
      if (entry.path === payload.path) {
        if (payload.durationMs != null) entry.duration_ms = payload.durationMs;
        entry.trim_start_ms = null;
        entry.trim_end_ms = null;
      }
    }
  }
  await refreshListing();
}

async function loadTranscriptFor(target: DirEntry): Promise<Transcript | null> {
  let t = transcript.value;
  if (!t || (selectedEntry.value && selectedEntry.value.path !== target.path)) {
    if (target.cache_key) t = await api.historyLoad(target.cache_key);
  }
  return t ?? null;
}

function isAndroid(): boolean {
  return /Android/i.test(navigator.userAgent);
}

async function onShare(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target) return;
  const t = await loadTranscriptFor(target);
  if (!t) {
    await message("Transcribe this file first to enable sharing.", {
      title: "Share",
      kind: "info",
    });
    return;
  }
  const stem = target.name.replace(/\.[^.]+$/, "");
  if (isAndroid()) {
    try {
      const text = await api.formatTranscript(t, "txt");
      await api.shareTranscript(stem, text);
      return;
    } catch (e) {
      error.value = String(e);
      return;
    }
  }
  await openExport(target);
}

async function openExport(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || renaming.value || exporting.value) return;
  const t = await loadTranscriptFor(target);
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
  const t = await loadTranscriptFor(target);
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
    const content = await api.formatTranscript(t, fmt);
    await writeTextFile(dest, content);
  } catch (e) {
    error.value = String(e);
  }
}

const selectedProgress = computed(() =>
  selectedEntry.value ? (progressByPath.value[selectedEntry.value.path] ?? null) : null,
);
</script>

<template>
  <StorageGate v-if="showStorageGate" @granted="onStorageGranted" @skipped="onStorageSkipped" />
  <SetupGate
    v-else-if="!essentialsReady"
    :essential-ids="essentialIds"
    :models="models"
    :progress="essentialProgress"
    :errors="essentialErrors"
    :runtimes="essentialRuntimes"
  />
  <div class="h-full flex flex-col bg-background text-on-background overflow-hidden">
    <AppHeader
      v-model:tab="tab"
      v-model:log-retain="logRetain"
      v-model:log-auto="logAuto"
      :show-transcribe-actions="tab === 'transcribe'"
      :show-log-controls="tab === 'logs'"
      :pending-count="untranscribedEntries.length"
      :queue-active="queueActive"
      :queue-done="queueDone"
      :queue-total="queueTotal"
      @transcribe-all="transcribeAll"
      @pick-audio="pickAudio"
      @log-refresh="logViewerRef?.refresh()"
      @log-clear="logViewerRef?.clear()"
    />

    <Recorder
      v-if="listing?.path"
      ref="recorderRef"
      :workdir="listing.path"
      :headless="true"
      @saved="onRecordingSaved"
    />

    <StatusStrip
      v-if="tab === 'transcribe'"
      :recording="recorderRef?.recording ?? false"
      :rec-elapsed="recorderRef?.elapsed ?? ''"
      :status="status"
      :selected-entry="selectedEntry"
      :progress="selectedProgress"
      :transcript="transcript"
      :audio-count="audioEntries.length"
      :transcribed-count="transcribedCount"
    />

    <main
      class="flex-1 flex flex-col md:flex-row overflow-hidden min-h-0"
      @click="fileListRef?.closeMenus()"
    >
      <template v-if="tab === 'transcribe'">
        <section
          class="flex-1 flex flex-col overflow-hidden bg-surface-container-lowest relative"
          :class="dragOver ? 'ring-2 ring-primary ring-inset' : ''"
        >
          <div
            v-if="queueActive"
            class="flex items-center gap-xs px-margin py-unit border-b border-outline-variant/40 shrink-0 font-mono text-labelSmall text-secondary"
          >
            <span class="w-1.5 h-1.5 rounded-full bg-secondary animate-pulse"></span>
            queue {{ queueDone + 1 }}/{{ queueTotal }}
          </div>

          <ErrorBanner v-if="error" icon="error" layout="inline" class="m-margin">
            <span class="wrap-break-word text-labelMedium">{{ error }}</span>
            <template #actions>
              <Button shape="link" @click="tab = 'logs'">View log</Button>
              <Button
                variant="ghost"
                shape="icon"
                icon="close"
                :icon-size="18"
                aria-label="Dismiss error"
                @click="error = null"
              />
            </template>
          </ErrorBanner>

          <div
            v-if="selectedPaths.size > 0"
            class="flex items-center gap-xs px-margin py-xs border-b border-outline-variant/40 shrink-0 bg-surface-container-low font-mono text-labelSmall"
          >
            <span class="text-on-surface-variant">{{ selectedPaths.size }} selected</span>
            <span class="text-outline-variant">•</span>
            <Button variant="ghost" shape="link" @click="bulkTranscribe">Transcribe</Button>
            <span class="text-outline-variant">•</span>
            <Button variant="ghost" shape="link" @click="bulkDelete">Delete</Button>
            <span class="text-outline-variant">•</span>
            <Button variant="ghost" shape="link" @click="clearSelection">Clear</Button>
          </div>

          <div class="flex-1 flex flex-col overflow-hidden px-xs md:px-md py-md">
            <div
              class="flex-1 flex flex-col overflow-hidden bg-surface-container/40 rounded-xl border border-outline-variant/40"
            >
              <div class="flex-1 overflow-y-auto scroll-overlay">
                <FileList
                  ref="fileListRef"
                  :entries="audioEntries"
                  :selected-path="selectedPath"
                  :busy="busy"
                  :progress-by-path="progressByPath"
                  :auto-renaming-path="autoRenamingPath"
                  :drag-over="dragOver"
                  :has-listing="!!listing"
                  @choose="chooseEntry"
                  @view="viewEntry"
                  @transcribe="runTranscribe"
                  @stop="stopTranscribe"
                  @trim="openTrim"
                  @auto-rename="autoRename"
                  @rename="openRename"
                  @share="onShare"
                  @export="openExport"
                  @redo-diarize="openRedoDiarize"
                  @delete="deleteEntry"
                  :selected-paths="selectedPaths"
                  @toggle-select="toggleSelect"
                  @range-select="rangeSelect"
                />
              </div>
            </div>
          </div>

          <TranscriptPanel
            v-if="transcript"
            :transcript="transcript"
            :cache-key="selectedEntry?.cache_key ?? null"
            @close="closeTranscript"
            @rename-speaker="onRenameSpeaker"
          />
        </section>

        <ConfigPanel
          v-if="config && listing?.path"
          :config="config"
          :sys="sys"
          :models="models"
          :workdir="listing.path"
          :selected-entry="selectedEntry"
          :progress="selectedProgress"
          :transcript="transcript"
          :status="status"
          :recording="recorderRef?.recording ?? false"
          :rec-elapsed="recorderRef?.elapsed ?? ''"
          @models-changed="(m) => (models = m)"
          @rec-start="recorderRef?.start()"
          @rec-stop="recorderRef?.stop()"
        />
      </template>

      <Settings v-else-if="tab === 'settings'" />
      <LogViewer
        v-else-if="tab === 'logs'"
        ref="logViewerRef"
        v-model:retain="logRetain"
        v-model:auto="logAuto"
      />
    </main>

    <BottomNav v-model="tab" />

    <RenameDialog v-model:open="renaming" v-model:value="renameValue" @commit="commitRename" />
    <ExportDialog v-model:open="exporting" v-model:format="exportFormat" @commit="commitExport" />
    <RedoDiarizeDialog
      v-model:open="redoDiarizeOpen"
      v-model:diarizer="redoDiarizeDiarizer"
      v-model:speakers="redoDiarizeSpeakers"
      :sys="sys"
      @commit="commitRedoDiarize"
    />
    <TrimDialog
      :target="trimTarget"
      @close="trimTarget = null"
      @saved="onTrimSaved"
      @error="(msg) => (error = msg)"
    />
  </div>
</template>
