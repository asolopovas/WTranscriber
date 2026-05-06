<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { open, save, confirm, message } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api } from "./api";
import type {
  Config,
  DirEntry,
  DirListing,
  ExportFormat,
  ModelInfo,
  Transcript,
} from "./types";
import ModelManager from "./components/ModelManager.vue";
import History from "./components/History.vue";
import Settings from "./components/Settings.vue";
import LogViewer from "./components/LogViewer.vue";

type Tab = "transcribe" | "models" | "history" | "settings" | "logs";

const tab = ref<Tab>("transcribe");
const version = ref("");
const config = ref<Config | null>(null);
const models = ref<ModelInfo[]>([]);
const listing = ref<DirListing | null>(null);
const selectedPath = ref<string>("");
const transcript = ref<Transcript | null>(null);
const status = ref<"idle" | "running" | "renaming" | "error">("idle");
const error = ref<string | null>(null);
const dragOver = ref(false);
const saveState = ref<"idle" | "saving" | "saved">("idle");
const busy = ref<Record<string, boolean>>({});
const dialogOpen = ref(false);

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
  { id: "history", label: "History" },
  { id: "models", label: "Models" },
  { id: "logs", label: "Logs" },
  { id: "settings", label: "Settings" },
];

const engineOptions = [
  { value: "whisper-onnx", label: "Whisper (ONNX)" },
  { value: "zipformer", label: "Zipformer" },
  { value: "parakeet", label: "Parakeet (NeMo)" },
  { value: "canary", label: "Canary" },
  { value: "nemo-ctc", label: "NeMo CTC" },
] as const;

const languageOptions = ["auto", "en", "de", "fr", "es", "it", "pt", "ru", "uk", "zh", "ja", "ko"];

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

const selectedEntry = computed<DirEntry | null>(() => {
  if (!listing.value || !selectedPath.value) return null;
  return listing.value.entries.find((e) => e.path === selectedPath.value) ?? null;
});

const audioEntries = computed<DirEntry[]>(() =>
  listing.value ? listing.value.entries.filter((e) => e.is_audio) : [],
);

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

async function addPathsToWorkdir(paths: string[]) {
  if (!listing.value) return;
  const dir = listing.value.path;
  let lastAdded = "";
  for (const p of paths) {
    if (!hasAudioExt(p)) continue;
    try {
      lastAdded = await api.addToWorkdir(p, dir);
    } catch (e) {
      error.value = String(e);
    }
  }
  await refreshListing();
  if (lastAdded) selectedPath.value = lastAdded;
}

function hasAudioExt(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return audioExtensions.includes(ext);
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
  await reload();
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
onUnmounted(() => unlistenDrop?.());

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
  selectedPath.value = target.path;
  status.value = "running";
  error.value = null;
  busy.value = { ...busy.value, [target.path]: true };
  try {
    transcript.value = await api.transcribeFile(target.path, config.value);
    status.value = "idle";
    await refreshListing();
  } catch (e) {
    error.value = String(e);
    status.value = "error";
  } finally {
    const next = { ...busy.value };
    delete next[target.path];
    busy.value = next;
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
    await message("Transcribe first to enable auto-rename.", { title: "Auto-rename", kind: "info" });
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

async function openExport(entry?: DirEntry) {
  const target = entry ?? selectedEntry.value;
  if (!target) return;
  if (renaming.value || exporting.value) return;
  let t = transcript.value;
  if (!t || (selectedEntry.value && selectedEntry.value.path !== target.path)) {
    if (target.cache_key) t = await api.historyLoad(target.cache_key);
  }
  if (!t) {
    await message("Transcribe this file first to enable export.", { title: "Export", kind: "info" });
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

function openHistoryItem(t: Transcript) {
  transcript.value = t;
  selectedPath.value = "";
  tab.value = "transcribe";
}

const fieldClass =
  "w-full bg-surface-container-high border border-outline-variant/60 text-on-surface text-bodyMedium px-md py-xs rounded-lg appearance-none focus:outline-none focus:border-primary transition-colors";
</script>

<template>
  <div class="h-screen flex flex-col bg-background text-on-background overflow-hidden">
    <header class="flex justify-between items-center w-full px-margin h-16 shrink-0 border-b border-outline-variant/40 bg-surface">
      <div class="flex items-center gap-xs">
        <span class="material-symbols-outlined text-primary text-[24px]">graphic_eq</span>
        <span class="font-mono tracking-tighter font-bold text-primary text-labelMedium ml-xs uppercase">wt</span>
      </div>
      <nav class="flex items-center gap-xl h-full">
        <button
          v-for="t in tabs"
          :key="t.id"
          @click="tab = t.id"
          class="h-full flex items-center text-titleSmall border-b-2 px-unit transition-colors"
          :class="tab === t.id
            ? 'border-primary text-on-surface'
            : 'border-transparent text-on-surface-variant hover:text-on-surface'"
        >
          {{ t.label }}
        </button>
      </nav>
      <div class="flex items-center gap-xs text-on-surface-variant">
        <span class="font-mono text-labelSmall">v{{ version }}</span>
        <span class="material-symbols-outlined text-[20px]">more_vert</span>
      </div>
    </header>

    <main class="flex-1 flex overflow-hidden">
      <template v-if="tab === 'transcribe'">
        <section
          class="flex-1 flex flex-col overflow-hidden bg-surface relative"
          :class="dragOver ? 'ring-2 ring-primary ring-inset' : ''"
        >
          <div class="flex items-center gap-xs px-margin h-12 border-b border-outline-variant/40 shrink-0">
            <span class="material-symbols-outlined text-on-surface-variant text-[18px]">folder</span>
            <span class="font-mono text-labelMedium text-on-surface truncate" :title="listing?.path">
              {{ listing?.path ?? "—" }}
            </span>
            <button
              class="text-on-surface-variant hover:text-on-surface transition-colors"
              @click="refreshListing"
              title="Refresh"
            >
              <span class="material-symbols-outlined text-[18px]">refresh</span>
            </button>
            <div class="flex-1"></div>
            <button
              class="px-md py-unit rounded-full border border-outline-variant text-on-surface text-labelMedium hover:bg-surface-container-high transition-colors flex items-center gap-unit"
              @click="pickFolder"
              title="Change working folder"
            >
              <span class="material-symbols-outlined text-[16px]">folder_open</span>
              Change
            </button>
            <button
              class="px-md py-unit rounded-full bg-primary text-on-primary text-labelMedium hover:bg-primary-fixed-dim transition-colors flex items-center gap-unit"
              @click="pickAudio"
              title="Add audio file(s) to working folder"
            >
              <span class="material-symbols-outlined text-[16px]">add</span>
              Add audio
            </button>
          </div>

          <div v-if="error" class="m-margin p-md rounded-lg bg-error-container/30 border border-error/40 text-error text-bodyMedium flex items-start gap-xs">
            <span class="material-symbols-outlined text-[18px] mt-[1px] shrink-0">error</span>
            <span class="flex-1 break-words font-mono text-labelMedium">{{ error }}</span>
            <button class="text-titleSmall underline hover:opacity-80 shrink-0" @click="tab = 'logs'">View log</button>
            <button class="material-symbols-outlined text-[18px] hover:opacity-70" @click="error = null">close</button>
          </div>

          <div class="flex-1 overflow-y-auto scroll-thin">
            <div v-if="!listing || audioEntries.length === 0" class="h-full flex flex-col items-center justify-center gap-md text-center px-xl text-on-surface-variant">
              <span class="material-symbols-outlined text-[48px]" :class="dragOver ? 'text-primary' : 'text-outline-variant'">
                {{ dragOver ? "download" : "library_music" }}
              </span>
              <p class="text-bodyMedium">{{ dragOver ? "Drop to add" : "No audio in this folder" }}</p>
              <p class="font-mono text-labelSmall text-outline">Drag files here or click Add audio</p>
            </div>

            <table v-else class="w-full text-bodyMedium">
              <thead class="sticky top-0 bg-surface z-10 border-b border-outline-variant/40">
                <tr class="text-left font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">
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
                    <span class="material-symbols-outlined text-[20px] text-on-surface-variant">graphic_eq</span>
                  </td>
                  <td class="px-xs py-xs truncate max-w-0">
                    <span class="text-on-surface">{{ entry.name }}</span>
                  </td>
                  <td class="px-xs py-xs font-mono text-labelMedium text-on-surface-variant">
                    {{ entry.duration_ms ? fmt(entry.duration_ms) : "—" }}
                  </td>
                  <td class="px-xs py-xs font-mono text-labelMedium text-on-surface-variant">
                    {{ fmtBytes(entry.size_bytes) }}
                  </td>
                  <td class="px-xs py-xs">
                    <span
                      v-if="busy[entry.path]"
                      class="font-mono text-labelSmall text-secondary flex items-center gap-unit"
                    >
                      <span class="material-symbols-outlined text-[14px] animate-pulse">graphic_eq</span>
                      transcribing
                    </span>
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
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-primary transition-colors"
                        :disabled="busy[entry.path]"
                        title="Transcribe"
                        @click="runTranscribe(entry)"
                      >play_arrow</button>
                      <button
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-secondary transition-colors"
                        title="Auto-rename (AI)"
                        @click="autoRename(entry)"
                      >auto_awesome</button>
                      <button
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-on-surface transition-colors"
                        title="Rename"
                        @click="openRename(entry)"
                      >drive_file_rename_outline</button>
                      <button
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-surface-container-highest text-on-surface-variant hover:text-on-surface transition-colors"
                        title="Export transcript"
                        :disabled="!entry.cache_key"
                        :class="!entry.cache_key ? 'opacity-30 cursor-not-allowed' : ''"
                        @click="openExport(entry)"
                      >download</button>
                      <button
                        class="material-symbols-outlined text-[18px] p-unit rounded hover:bg-error-container/40 text-on-surface-variant hover:text-error transition-colors"
                        title="Delete"
                        @click="deleteEntry(entry)"
                      >delete</button>
                    </div>
                  </td>
                </tr>
              </tbody>
            </table>
          </div>

          <div v-if="transcript" class="border-t border-outline-variant/40 max-h-[40%] overflow-y-auto scroll-thin p-margin shrink-0">
            <div class="flex items-center justify-between mb-md">
              <h3 class="text-titleSmall text-on-surface flex items-center gap-xs">
                <span class="material-symbols-outlined text-primary text-[18px]">subtitles</span>
                Transcript
                <span v-if="selectedPath" class="font-mono text-labelSmall text-on-surface-variant">— {{ basename(selectedPath) }}</span>
              </h3>
              <button
                class="text-on-surface-variant hover:text-on-surface text-titleSmall"
                @click="transcript = null"
              >
                <span class="material-symbols-outlined text-[18px]">close</span>
              </button>
            </div>
            <article
              v-for="(u, i) in transcript.utterances"
              :key="i"
              class="flex gap-md items-start group hover:bg-surface-container-high/30 -mx-xs px-xs py-unit rounded transition-colors"
            >
              <span class="font-mono text-labelSmall text-secondary w-20 shrink-0 pt-unit">{{ fmt(u.start_ms) }}</span>
              <div class="flex-1 min-w-0">
                <div v-if="u.speaker" class="font-mono text-labelSmall text-primary mb-unit">{{ u.speaker }}</div>
                <p class="text-bodyMedium text-on-surface-variant group-hover:text-on-surface transition-colors leading-relaxed">{{ u.text }}</p>
              </div>
            </article>
          </div>
        </section>

        <aside class="w-[340px] bg-surface-container border-l border-outline-variant/40 flex flex-col h-full shrink-0 overflow-y-auto scroll-thin">
          <div v-if="config" class="p-margin space-y-xl">
            <div>
              <div class="flex items-center justify-between mb-md">
                <h3 class="text-titleSmall text-on-surface">Configuration</h3>
                <span
                  class="font-mono text-labelSmall flex items-center gap-unit"
                  :class="saveState === 'saving' ? 'text-secondary' : saveState === 'saved' ? 'text-tertiary' : 'text-outline'"
                >
                  <span
                    class="w-1.5 h-1.5 rounded-full"
                    :class="saveState === 'saving' ? 'bg-secondary animate-pulse' : saveState === 'saved' ? 'bg-tertiary' : 'bg-outline-variant'"
                  ></span>
                  {{ saveState === "saving" ? "saving" : saveState === "saved" ? "saved" : "synced" }}
                </span>
              </div>

              <div class="space-y-md">
                <label class="block">
                  <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">Engine</span>
                  <select v-model="config.engine" :class="[fieldClass, 'mt-unit']">
                    <option v-for="o in engineOptions" :key="o.value" :value="o.value">{{ o.label }}</option>
                  </select>
                </label>

                <label class="block">
                  <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">Model</span>
                  <select v-model="config.model" :class="[fieldClass, 'mt-unit']">
                    <option v-if="!asrModels.length" :value="config.model" disabled>
                      No installed models — open Models tab
                    </option>
                    <option v-for="m in asrModels" :key="m.id" :value="m.id">{{ m.display_name }}</option>
                  </select>
                </label>

                <div class="grid grid-cols-2 gap-md">
                  <label class="block">
                    <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">Language</span>
                    <select v-model="config.language" :class="[fieldClass, 'mt-unit']">
                      <option v-for="l in languageOptions" :key="l" :value="l">{{ l }}</option>
                    </select>
                  </label>
                  <label class="block">
                    <span class="font-mono text-labelSmall text-on-surface-variant uppercase tracking-wide">Device</span>
                    <select v-model="config.device" :class="[fieldClass, 'mt-unit']">
                      <option value="cpu">CPU</option>
                      <option value="cuda">CUDA</option>
                    </select>
                  </label>
                </div>

                <div class="flex items-center justify-between bg-surface-container-high p-md rounded-lg border border-outline-variant/40">
                  <div>
                    <div class="text-bodyMedium text-on-surface">Diarize speakers</div>
                    <div class="font-mono text-labelSmall text-on-surface-variant">
                      {{ config.diarize ? (config.speakers ? `${config.speakers} speakers` : "auto-detect") : "disabled" }}
                    </div>
                  </div>
                  <button
                    type="button"
                    class="w-10 h-6 rounded-full relative shrink-0 transition-colors"
                    :class="config.diarize ? 'bg-primary' : 'bg-surface-container-highest border border-outline-variant'"
                    @click="config.diarize = !config.diarize"
                  >
                    <span
                      class="absolute top-1 w-4 h-4 rounded-full transition-all"
                      :class="config.diarize ? 'right-1 bg-on-primary' : 'left-1 bg-outline'"
                    ></span>
                  </button>
                </div>
              </div>
            </div>

            <div>
              <h3 class="text-titleSmall text-on-surface mb-md">Selection</h3>
              <div class="bg-surface-container-high p-md rounded-lg space-y-xs font-mono text-labelMedium">
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">File</span>
                  <span class="text-on-surface truncate ml-md max-w-[180px]" :title="selectedEntry?.name">
                    {{ selectedEntry?.name ?? "—" }}
                  </span>
                </div>
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Status</span>
                  <span :class="status === 'error' ? 'text-error' : status === 'idle' ? 'text-tertiary' : 'text-secondary'">
                    {{ status === "idle" && transcript ? "ready" : status }}
                  </span>
                </div>
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Duration</span>
                  <span class="text-on-surface">{{ transcript ? fmtLong(transcript.duration_ms) : "—" }}</span>
                </div>
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Utterances</span>
                  <span class="text-on-surface">{{ transcript?.utterances.length ?? 0 }}</span>
                </div>
                <div class="flex justify-between items-center">
                  <span class="text-on-surface-variant">Speakers</span>
                  <span class="text-primary">{{ transcript?.speakers_detected ?? 0 }}</span>
                </div>
              </div>
            </div>
          </div>
        </aside>
      </template>

      <ModelManager v-else-if="tab === 'models'" />
      <Settings v-else-if="tab === 'settings'" />
      <History v-else-if="tab === 'history'" @open="openHistoryItem" />
      <LogViewer v-else-if="tab === 'logs'" />
    </main>

    <div
      v-if="renaming"
      class="fixed inset-0 z-40 bg-black/50 flex items-center justify-center"
      @click.self="renaming = false"
    >
      <div class="bg-surface-container rounded-xl border border-outline-variant/40 p-margin w-[420px] max-w-[90vw] space-y-md">
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
          >Cancel</button>
          <button
            class="px-md py-xs rounded-full bg-primary text-on-primary text-titleSmall hover:bg-primary-fixed-dim"
            @click="commitRename"
          >Rename</button>
        </div>
      </div>
    </div>

    <div
      v-if="exporting"
      class="fixed inset-0 z-40 bg-black/50 flex items-center justify-center"
      @click.self="exporting = false"
    >
      <div class="bg-surface-container rounded-xl border border-outline-variant/40 p-margin w-[420px] max-w-[90vw] space-y-md">
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
          >Cancel</button>
          <button
            class="px-md py-xs rounded-full bg-primary text-on-primary text-titleSmall hover:bg-primary-fixed-dim"
            @click="commitExport"
          >Save…</button>
        </div>
      </div>
    </div>
  </div>
</template>
