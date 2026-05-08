<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { open, save, confirm, message } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, events } from "@/api";
import type {
  Config,
  DirEntry,
  DirListing,
  ExportFormat,
  FileProgress,
  ModelInfo,
  SystemInfo,
  TranscribeProgress,
  Transcript,
} from "@/types";
import Settings from "@components/Settings.vue";
import LogViewer from "@components/LogViewer.vue";
import SetupGate from "@components/SetupGate.vue";
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
import TrimDialog from "@components/dialogs/TrimDialog.vue";
import { audioExtensions, basenameOf, hasAudioExt } from "@utils/audio";
import { useDebouncedSave } from "@composables/useDebouncedSave";
import { recordOmit, recordSet } from "@composables/records";

const tab = ref<Tab>("transcribe");
const version = ref("");
const sys = ref<SystemInfo | null>(null);
const config = ref<Config | null>(null);
const models = ref<ModelInfo[]>([]);
const listing = ref<DirListing | null>(null);
const selectedPath = ref<string>("");
const selectedPaths = ref(new Set<string>());
let lastRangeAnchor: string | null = null;
const transcript = ref<Transcript | null>(null);
const status = ref<"idle" | "running" | "renaming" | "error">("idle");
const error = ref<string | null>(null);
const dragOver = ref(false);
const busy = ref<Record<string, boolean>>({});
const progressByPath = ref<Record<string, TranscribeProgress>>({});

const essentialIds = ref<string[]>([]);
const essentialProgress = ref<Record<string, FileProgress>>({});
const essentialErrors = ref<Record<string, true>>({});
const essentialsForceReady = ref(false);
const essentialsReady = computed(() => {
  if (essentialsForceReady.value) return true;
  if (!essentialIds.value.length) return true;
  return essentialIds.value.every(
    (id) => models.value.find((x) => x.id === id)?.status === "installed",
  );
});

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

  const tryAdd = async (p: string): Promise<string> => {
    let eRaw: unknown;
    try {
      return await api.addToWorkdir(p, dir);
    } catch (e) {
      eRaw = e;
    }
    if (!sys.value?.is_mobile) {
      throw eRaw instanceof Error ? eRaw : new Error(String(eRaw));
    }
    try {
      const bytes = await readFile(p);
      if (bytes.byteLength > 200 * 1024 * 1024) {
        throw new Error("file exceeds 200 MB limit for in-process copy");
      }
      await yieldToUI();
      return await api.saveRecording(dir, basenameOf(p), bytes);
    } catch (e2) {
      throw new Error(`${eRaw} / ${e2}`);
    }
  };

  const audioPaths = paths.filter(hasAudioExt).filter((p) => !entries.some((e) => e.path === p));
  if (audioPaths.length > 200) {
    audioPaths.splice(200);
    error.value = "only the first 200 files were queued";
  }

  for (const p of audioPaths) {
    entries.push({
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
    });
  }

  void (async () => {
    const copyResults: PromiseSettledResult<string>[] = [];
    for (let i = 0; i < audioPaths.length; i += 3) {
      await yieldToUI();
      const chunk = await Promise.allSettled(
        audioPaths.slice(i, i + 3).map(async (p) => {
          const destPath = await tryAdd(p);
          if (listing.value) {
            const stub = listing.value.entries.find((e) => e.path === p);
            if (stub) {
              stub.name = basenameOf(destPath);
              stub.path = destPath;
            }
          }
          return destPath;
        }),
      );
      copyResults.push(...chunk);
    }

    let lastDest: string | null = null;
    const addedPaths = new Set<string>();
    for (let i = 0; i < copyResults.length; i++) {
      const r = copyResults[i];
      if (r.status === "fulfilled" && r.value != null) {
        lastDest = r.value;
        addedPaths.add(r.value);
      } else if (r.status === "rejected") {
        error.value = String(r.reason);
        if (listing.value) {
          const idx = listing.value.entries.findIndex((e) => e.path === audioPaths[i]);
          if (idx !== -1) listing.value.entries.splice(idx, 1);
        }
      }
    }

    await refreshListing();
    if (lastDest !== null) selectedPath.value = lastDest;

    if (!listing.value) return;
    const toProbe = listing.value.entries.filter(
      (e) => e.is_audio && e.duration_ms === null && addedPaths.has(e.path),
    );
    for (let i = 0; i < toProbe.length; i += 10) {
      await yieldToUI();
      await Promise.allSettled(
        toProbe.slice(i, i + 10).map((e) =>
          api.probeDuration(e.path).then((ms) => {
            e.duration_ms = ms ?? null;
          }),
        ),
      );
    }
  })();
}

async function onRecordingSaved(path: string) {
  await refreshListing();
  selectedPath.value = path;
  transcript.value = null;
  error.value = null;
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
  unlisten.push(
    await events.onTranscribeProgress((p) => recordSet(progressByPath, p.path, p)),
    await events.onModelProgress((p) => {
      if (essentialIds.value.includes(p.id)) recordSet(essentialProgress, p.id, p);
    }),
    await events.onModelDone((id) => {
      if (id) recordOmit(essentialErrors, id);
      void refreshModels(id);
    }),
    await events.onModelError((id) => {
      if (id) recordSet(essentialErrors, id, true);
      void refreshModels();
    }),
    await events.onEssentialsDone(() => {
      essentialsForceReady.value = true;
      void refreshModels();
    }),
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
      selectedPaths.value = new Set(audioEntries.value.map((en) => en.path));
    } else if (e.key === "Escape" && selectedPaths.value.size > 0) {
      clearSelection();
    } else if ((e.key === "Delete" || e.key === "Backspace") && selectedPaths.value.size > 0) {
      e.preventDefault();
      void bulkDelete();
    }
  }
  document.addEventListener("keydown", onKeyDown);
  unlisten.push(() => document.removeEventListener("keydown", onKeyDown));
});

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
  if (!selectedModelInstalled.value) {
    error.value = `Model "${selectedAsrModel.value?.display_name ?? config.value.model}" is not installed. Download it in Configuration.`;
    tab.value = "transcribe";
    return;
  }
  selectedPath.value = target.path;
  status.value = "running";
  error.value = null;
  recordSet(busy, target.path, true);
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
    recordOmit(busy, target.path);
    recordOmit(progressByPath, target.path);
  }
}

async function stopTranscribe(entry: DirEntry) {
  await api.cancelTranscribe(entry.path);
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
      await runTranscribe(entry);
      queueDone.value += 1;
    }
  } finally {
    queueActive.value = false;
    queueTotal.value = 0;
    queueDone.value = 0;
  }
}

function toggleSelect(path: string) {
  const next = new Set(selectedPaths.value);
  if (next.has(path)) next.delete(path);
  else next.add(path);
  selectedPaths.value = next;
  lastRangeAnchor = path;
}

function rangeSelect(anchor: string) {
  if (anchor === "__all__") {
    if (selectedPaths.value.size === audioEntries.value.length) {
      selectedPaths.value = new Set();
    } else {
      selectedPaths.value = new Set(audioEntries.value.map((e) => e.path));
    }
    return;
  }
  const paths = audioEntries.value.map((e) => e.path);
  const anchorIdx = paths.indexOf(anchor);
  const from = lastRangeAnchor ? paths.indexOf(lastRangeAnchor) : anchorIdx;
  if (anchorIdx < 0 || from < 0) return;
  const lo = Math.min(from, anchorIdx);
  const hi = Math.max(from, anchorIdx);
  const next = new Set(selectedPaths.value);
  for (let i = lo; i <= hi; i++) next.add(paths[i]);
  selectedPaths.value = next;
  lastRangeAnchor = anchor;
}

function clearSelection() {
  selectedPaths.value = new Set();
  lastRangeAnchor = null;
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
async function autoRename(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || !target.is_audio || autoRenamingPath.value) return;
  autoRenamingPath.value = target.path;
  try {
    let t = transcript.value;
    if (!t && target.cache_key) t = await api.historyLoad(target.cache_key);
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

const trimTarget = ref<DirEntry | null>(null);
function openTrim(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || !target.is_audio) return;
  if (trimTarget.value || renaming.value || exporting.value) return;
  trimTarget.value = target;
}
async function onTrimSaved() {
  trimTarget.value = null;
  await refreshListing();
}

async function openExport(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target || renaming.value || exporting.value) return;
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

const selectedProgress = computed(() =>
  selectedEntry.value ? (progressByPath.value[selectedEntry.value.path] ?? null) : null,
);
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
    <AppHeader
      v-model:tab="tab"
      :version="version"
      :show-transcribe-actions="tab === 'transcribe'"
      :pending-count="untranscribedEntries.length"
      :queue-active="queueActive"
      :queue-done="queueDone"
      :queue-total="queueTotal"
      @transcribe-all="transcribeAll"
      @pick-audio="pickAudio"
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
          class="flex-1 flex flex-col overflow-hidden bg-surface relative"
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
            <span class="break-words text-labelMedium">{{ error }}</span>
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

          <div class="flex-1 overflow-y-auto scroll-thin">
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
              @transcribe="runTranscribe"
              @stop="stopTranscribe"
              @trim="openTrim"
              @auto-rename="autoRename"
              @rename="openRename"
              @export="openExport"
              @delete="deleteEntry"
              :selected-paths="selectedPaths"
              @toggle-select="toggleSelect"
              @range-select="rangeSelect"
            />
          </div>

          <TranscriptPanel v-if="transcript" :transcript="transcript" @close="closeTranscript" />
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
      <LogViewer v-else-if="tab === 'logs'" />
    </main>

    <BottomNav v-model="tab" />

    <RenameDialog v-model:open="renaming" v-model:value="renameValue" @commit="commitRename" />
    <ExportDialog v-model:open="exporting" v-model:format="exportFormat" @commit="commitExport" />
    <TrimDialog
      :target="trimTarget"
      @close="trimTarget = null"
      @saved="onTrimSaved"
      @error="(msg) => (error = msg)"
    />
  </div>
</template>
