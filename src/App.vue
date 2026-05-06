<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api } from "./api";
import type { Config, ModelInfo, Suggestion, Transcript } from "./types";
import ModelManager from "./components/ModelManager.vue";
import History from "./components/History.vue";
import Settings from "./components/Settings.vue";
import LogViewer from "./components/LogViewer.vue";

type Tab = "transcribe" | "models" | "history" | "settings" | "logs";

const tab = ref<Tab>("transcribe");
const version = ref("");
const config = ref<Config | null>(null);
const models = ref<ModelInfo[]>([]);
const transcript = ref<Transcript | null>(null);
const suggestion = ref<Suggestion | null>(null);
const status = ref<"idle" | "running" | "renaming" | "error">("idle");
const error = ref<string | null>(null);
const sourcePath = ref<string>("");
const dragOver = ref(false);
const saveState = ref<"idle" | "saving" | "saved">("idle");

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

const asrModels = computed(() =>
  models.value.filter((m) => m.family === "asr" && m.status === "installed"),
);

const audioExtensions = ["wav", "mp3", "ogg", "m4a", "flac"];

async function reload() {
  config.value = await api.loadConfig();
  models.value = await api.listModels();
}

onMounted(async () => {
  version.value = await api.appVersion();
  await reload();
  unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
    if (tab.value !== "transcribe") return;
    if (event.payload.type === "over") {
      dragOver.value = true;
    } else if (event.payload.type === "leave") {
      dragOver.value = false;
    } else if (event.payload.type === "drop") {
      dragOver.value = false;
      const path = event.payload.paths?.[0];
      if (path && hasAudioExt(path)) void runTranscribe(path);
      else if (path) error.value = `Unsupported file type: ${path}`;
    }
  });
});

let unlistenDrop: (() => void) | null = null;
onUnmounted(() => unlistenDrop?.());

function hasAudioExt(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return audioExtensions.includes(ext);
}

watch(tab, (t) => {
  if (t === "transcribe") void reload();
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

async function pickFile() {
  const selected = await open({
    multiple: false,
    filters: [{ name: "Audio", extensions: audioExtensions }],
  });
  if (typeof selected === "string") void runTranscribe(selected);
}

async function runTranscribe(path: string) {
  if (!config.value) return;
  sourcePath.value = path;
  status.value = "running";
  error.value = null;
  suggestion.value = null;
  try {
    transcript.value = await api.transcribeFile(path, config.value);
    status.value = "idle";
    if (config.value.auto_rename) {
      status.value = "renaming";
      try {
        suggestion.value = await api.suggestFilename(transcript.value);
      } catch (e) {
        error.value = `auto-rename failed: ${String(e)}`;
      } finally {
        status.value = "idle";
      }
    }
  } catch (e) {
    error.value = String(e);
    status.value = "error";
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

function basename(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

function openHistoryItem(t: Transcript) {
  transcript.value = t;
  suggestion.value = null;
  sourcePath.value = "";
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
          class="flex-1 flex flex-col p-margin overflow-hidden bg-surface relative"
          :class="dragOver ? 'ring-2 ring-primary ring-inset' : ''"
        >
          <div class="flex items-center justify-between mb-margin">
            <div class="flex items-center gap-xs text-on-surface-variant min-w-0">
              <span class="material-symbols-outlined text-[18px] shrink-0">folder_open</span>
              <span class="font-mono text-labelMedium truncate">{{ sourcePath ? basename(sourcePath) : "no file selected" }}</span>
            </div>
            <div class="flex gap-xs shrink-0">
              <button
                :disabled="status === 'running' || status === 'renaming'"
                @click="pickFile"
                class="bg-primary text-on-primary px-margin py-xs rounded-full font-medium text-titleSmall flex items-center gap-unit hover:bg-primary-fixed-dim transition-colors disabled:opacity-60 disabled:cursor-progress"
              >
                <span class="material-symbols-outlined text-[18px]">{{ status === 'running' ? 'hourglass_top' : status === 'renaming' ? 'auto_awesome' : 'upload_file' }}</span>
                <span>{{ status === "running" ? "Transcribing…" : status === "renaming" ? "Naming…" : "Pick audio" }}</span>
              </button>
            </div>
          </div>

          <div v-if="error" class="mb-margin p-md rounded-lg bg-error-container/30 border border-error/40 text-error text-bodyMedium flex items-start gap-xs">
            <span class="material-symbols-outlined text-[18px] mt-[1px] shrink-0">error</span>
            <span class="flex-1 break-words font-mono text-labelMedium">{{ error }}</span>
            <button class="text-titleSmall underline hover:opacity-80 shrink-0" @click="tab = 'logs'">View log</button>
            <button class="material-symbols-outlined text-[18px] hover:opacity-70" @click="error = null">close</button>
          </div>

          <div v-if="suggestion" class="mb-margin p-md rounded-lg bg-secondary-container/40 border border-secondary/30 flex items-center gap-xs text-bodyMedium">
            <span class="material-symbols-outlined text-secondary text-[18px]">auto_awesome</span>
            <span class="text-on-surface-variant">Suggested:</span>
            <code class="font-mono text-secondary bg-surface-container px-xs py-unit rounded">{{ suggestion.topic }}_{{ suggestion.stamp }}</code>
          </div>

          <div class="flex-1 overflow-y-auto pr-xs scroll-thin relative">
            <div
              v-if="!transcript && status !== 'running'"
              class="h-full flex flex-col items-center justify-center gap-md text-center px-xl"
            >
              <div
                class="w-full max-w-md border-2 border-dashed rounded-xl p-xl flex flex-col items-center gap-md transition-colors"
                :class="dragOver
                  ? 'border-primary bg-primary/10 text-primary'
                  : 'border-outline-variant text-on-surface-variant hover:border-outline'"
              >
                <span class="material-symbols-outlined text-[56px]" :class="dragOver ? 'text-primary' : 'text-outline-variant'">
                  {{ dragOver ? 'download' : 'graphic_eq' }}
                </span>
                <p class="text-bodyMedium">
                  {{ dragOver ? "Drop to transcribe" : "Drag an audio file here" }}
                </p>
                <p v-if="!dragOver" class="font-mono text-labelSmall text-outline">
                  {{ audioExtensions.map((e) => '.' + e).join(' · ') }}
                </p>
                <button
                  v-if="!dragOver"
                  @click="pickFile"
                  class="mt-xs px-md py-xs rounded-full border border-outline text-on-surface text-titleSmall hover:bg-surface-container-high transition-colors"
                >
                  or browse files
                </button>
              </div>
            </div>

            <div v-else-if="status === 'running'" class="h-full flex flex-col items-center justify-center gap-md text-on-surface-variant">
              <span class="material-symbols-outlined text-[56px] text-primary animate-pulse">graphic_eq</span>
              <p class="text-bodyMedium">Transcribing {{ basename(sourcePath) }}…</p>
            </div>

            <div v-else class="space-y-md">
              <article
                v-for="(u, i) in transcript!.utterances"
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
              <h3 class="text-titleSmall text-on-surface mb-md">Session</h3>
              <div class="bg-surface-container-high p-md rounded-lg space-y-xs font-mono text-labelMedium">
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
  </div>
</template>
