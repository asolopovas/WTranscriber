<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { confirm } from "@tauri-apps/plugin-dialog";
import { api, events } from "../api";
import type { Config, FileProgress, ModelInfo } from "../types";

const config = ref<Config | null>(null);
const models = ref<ModelInfo[]>([]);
const status = ref<"idle" | "saving" | "saved" | "error">("idle");
const error = ref<string | null>(null);
const maintenanceStatus = ref<string | null>(null);
const modelProgress = ref<Record<string, FileProgress>>({});
const unlisten: (() => void)[] = [];

async function refreshModels() {
  models.value = await api.listModels();
}

async function installModel(id: string) {
  try {
    await api.installModel(id);
  } finally {
    delete modelProgress.value[id];
    await refreshModels();
  }
}

function fmtSize(bytes: number): string {
  if (!bytes) return "—";
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(2)} GB`;
  return `${(bytes / 1_048_576).toFixed(0)} MB`;
}

function pct(p?: FileProgress): number {
  if (!p || !p.total) return 0;
  const fileFrac = p.downloaded / p.total;
  return ((p.file_index + fileFrac) / p.file_count) * 100;
}

const groupedModels = computed(() => {
  const families: Record<string, ModelInfo[]> = { asr: [], diarizer: [], llm: [] };
  for (const m of models.value) families[m.family]?.push(m);
  return [
    { id: "asr", label: "ASR engines", icon: "graphic_eq", items: families.asr },
    { id: "diarizer", label: "Diarizers", icon: "groups", items: families.diarizer },
    { id: "llm", label: "Language models", icon: "model_training", items: families.llm },
  ].filter((g) => g.items.length);
});

onMounted(async () => {
  config.value = await api.loadConfig();
  await refreshModels();
  unlisten.push(
    await events.onModelProgress((p) => {
      modelProgress.value = { ...modelProgress.value, [p.id]: p };
    }),
    await events.onModelDone(refreshModels),
    await events.onModelError(refreshModels),
  );
});

onUnmounted(() => unlisten.forEach((u) => u()));

async function resetTranscriptCache() {
  const ok = await confirm("Clear saved transcript previews and cached transcription results?");
  if (!ok) return;
  const removed = await api.resetTranscriptCache();
  maintenanceStatus.value = `Transcript cache reset (${removed} files removed).`;
}

async function resetAudioCache() {
  const ok = await confirm("Clear converted audio cache files?");
  if (!ok) return;
  const removed = await api.resetAudioCache();
  maintenanceStatus.value = `Audio cache reset (${removed} files removed).`;
}

let saveTimer: ReturnType<typeof setTimeout> | null = null;
watch(
  config,
  (next) => {
    if (!next) return;
    if (saveTimer) clearTimeout(saveTimer);
    status.value = "saving";
    saveTimer = setTimeout(async () => {
      try {
        await api.saveConfig(next);
        status.value = "saved";
        error.value = null;
      } catch (e) {
        status.value = "error";
        error.value = String(e);
      }
    }, 250);
  },
  { deep: true },
);

const fieldClass =
  "w-full bg-surface-container-high border border-outline-variant/60 text-on-surface text-bodyMedium px-md py-xs rounded-lg appearance-none focus:outline-none focus:border-primary transition-colors";
</script>

<template>
  <main class="flex-1 overflow-y-auto p-xl bg-surface-container-lowest scroll-thin">
    <div class="max-w-5xl mx-auto flex flex-col gap-xl pb-xl">
      <div
        class="flex flex-col md:flex-row md:items-end justify-between gap-margin pb-md border-b border-outline-variant/50"
      >
        <div>
          <h1 class="text-[24px] leading-[32px] font-bold text-on-surface">Compute</h1>
          <p class="text-bodyMedium text-on-surface-variant mt-unit">
            Runtime device, local models, and storage maintenance.
          </p>
        </div>
        <div class="flex items-center gap-xs shrink-0 font-mono text-labelMedium">
          <span
            class="w-2 h-2 rounded-full"
            :class="
              status === 'saving'
                ? 'bg-secondary animate-pulse'
                : status === 'error'
                  ? 'bg-error'
                  : status === 'saved'
                    ? 'bg-tertiary'
                    : 'bg-outline-variant'
            "
          ></span>
          <span class="text-on-surface-variant uppercase tracking-wide">
            {{
              status === "saving"
                ? "saving…"
                : status === "saved"
                  ? "saved"
                  : status === "error"
                    ? "error"
                    : "synced"
            }}
          </span>
        </div>
      </div>

      <p v-if="error" class="text-error text-bodyMedium">{{ error }}</p>

      <div v-if="config" class="flex flex-col gap-margin">
        <section
          class="bg-surface-container rounded-xl border border-outline-variant/50 overflow-hidden"
        >
          <div
            class="p-margin border-b border-outline-variant/40 bg-surface-container-low flex items-center gap-xs"
          >
            <span class="material-symbols-outlined text-secondary">memory</span>
            <h2 class="text-titleMedium text-on-surface">Runtime</h2>
          </div>
          <div class="p-margin">
            <label class="flex flex-col gap-unit max-w-sm">
              <span class="text-titleSmall text-on-surface">Threads</span>
              <input
                v-model.number="config.threads"
                type="number"
                min="1"
                max="32"
                :class="fieldClass"
              />
              <span class="text-bodyMedium text-on-surface-variant"
                >CPU worker threads (1–32).</span
              >
            </label>
          </div>
        </section>

        <section
          v-for="g in groupedModels"
          :key="g.id"
          class="bg-surface-container rounded-xl border border-outline-variant/50 overflow-hidden"
        >
          <div
            class="p-margin border-b border-outline-variant/40 bg-surface-container-low flex items-center gap-xs"
          >
            <span class="material-symbols-outlined text-tertiary">{{ g.icon }}</span>
            <h2 class="text-titleMedium text-on-surface">{{ g.label }}</h2>
          </div>
          <table class="w-full text-left border-collapse">
            <thead>
              <tr class="border-b border-outline-variant/40 bg-surface-container-highest/40">
                <th class="px-margin py-md text-titleSmall text-on-surface-variant font-medium">
                  Name
                </th>
                <th
                  class="px-margin py-md text-titleSmall text-on-surface-variant font-medium w-28"
                >
                  Size
                </th>
                <th
                  class="px-margin py-md text-titleSmall text-on-surface-variant font-medium w-48"
                >
                  Status
                </th>
                <th
                  class="px-margin py-md text-titleSmall text-on-surface-variant font-medium text-right w-32"
                >
                  Actions
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="m in g.items"
                :key="m.id"
                class="border-b border-outline-variant/30 last:border-b-0 hover:bg-surface-container-high/40 transition-colors"
              >
                <td class="px-margin py-md align-top">
                  <div class="font-mono text-labelMedium text-on-surface">{{ m.id }}</div>
                  <div class="text-bodyMedium text-on-surface-variant mt-unit">
                    {{ m.description }}
                  </div>
                  <div
                    v-if="m.default_active"
                    class="font-mono text-labelSmall text-secondary mt-unit uppercase tracking-wide"
                  >
                    default
                  </div>
                </td>
                <td
                  class="px-margin py-md text-on-surface-variant align-top whitespace-nowrap"
                >
                  {{ fmtSize(m.size_bytes) }}
                </td>
                <td class="px-margin py-md align-top">
                  <div v-if="modelProgress[m.id]" class="flex flex-col gap-unit w-40">
                    <div class="h-1 bg-surface-container-highest rounded-full overflow-hidden">
                      <div
                        class="h-full bg-primary transition-all"
                        :style="{ width: pct(modelProgress[m.id]) + '%' }"
                      ></div>
                    </div>
                    <span class="font-mono text-labelSmall text-primary"
                      >{{ pct(modelProgress[m.id]).toFixed(0) }}% · file
                      {{ modelProgress[m.id].file_index + 1 }}/{{
                        modelProgress[m.id].file_count
                      }}</span
                    >
                  </div>
                  <span
                    v-else-if="m.status === 'installed'"
                    class="inline-flex items-center gap-unit bg-tertiary-container/30 text-tertiary border border-tertiary/30 px-xs py-unit rounded-full font-mono text-labelSmall"
                  >
                    <span class="w-2 h-2 rounded-full bg-tertiary"></span> Installed
                  </span>
                  <span
                    v-else-if="m.status === 'downloading'"
                    class="inline-flex items-center gap-unit bg-secondary-container/40 text-secondary border border-secondary/30 px-xs py-unit rounded-full font-mono text-labelSmall"
                  >
                    <span class="w-2 h-2 rounded-full bg-secondary animate-pulse"></span>
                    Downloading
                  </span>
                  <span
                    v-else
                    class="inline-flex items-center gap-unit border border-outline-variant text-on-surface-variant px-xs py-unit rounded-full font-mono text-labelSmall"
                  >
                    <span class="w-2 h-2 rounded-full bg-outline-variant"></span> Not installed
                  </span>
                </td>
                <td class="px-margin py-md text-right align-top">
                  <button
                    v-if="m.status === 'not_installed'"
                    class="px-md py-xs rounded-full bg-primary text-on-primary text-titleSmall hover:bg-primary-fixed-dim transition-colors inline-flex items-center gap-unit"
                    @click="installModel(m.id)"
                  >
                    <span class="material-symbols-outlined text-[16px]">download</span>
                    Install
                  </button>
                  <span
                    v-else-if="m.status === 'installed'"
                    class="text-outline material-symbols-outlined text-[20px] cursor-not-allowed"
                    title="Installed"
                    >check</span
                  >
                </td>
              </tr>
            </tbody>
          </table>
        </section>

        <section
          class="bg-surface-container rounded-xl border border-outline-variant/50 overflow-hidden"
        >
          <div
            class="p-margin border-b border-outline-variant/40 bg-surface-container-low flex items-center gap-xs"
          >
            <span class="material-symbols-outlined text-tertiary">cleaning_services</span>
            <h2 class="text-titleMedium text-on-surface">Maintenance</h2>
          </div>
          <div class="p-margin grid grid-cols-1 md:grid-cols-2 gap-margin">
            <div class="flex flex-col gap-md">
              <div>
                <h3 class="text-titleSmall text-on-surface">Transcript cache</h3>
                <p class="text-bodyMedium text-on-surface-variant">
                  Clears saved transcript previews and cached transcription results.
                </p>
              </div>
              <button
                type="button"
                class="px-md py-xs rounded-full border border-outline-variant text-on-surface text-titleSmall hover:bg-surface-container-high transition-colors inline-flex items-center gap-unit w-fit"
                @click="resetTranscriptCache"
              >
                <span class="material-symbols-outlined text-[18px]">delete_sweep</span>
                Reset transcript cache
              </button>
            </div>
            <div class="flex flex-col gap-md">
              <div>
                <h3 class="text-titleSmall text-on-surface">Audio cache</h3>
                <p class="text-bodyMedium text-on-surface-variant">
                  Clears converted WAV files created for non-WAV audio inputs.
                </p>
              </div>
              <button
                type="button"
                class="px-md py-xs rounded-full border border-outline-variant text-on-surface text-titleSmall hover:bg-surface-container-high transition-colors inline-flex items-center gap-unit w-fit"
                @click="resetAudioCache"
              >
                <span class="material-symbols-outlined text-[18px]">delete_sweep</span>
                Reset audio cache
              </button>
            </div>
            <p v-if="maintenanceStatus" class="md:col-span-2 text-bodyMedium text-tertiary">
              {{ maintenanceStatus }}
            </p>
          </div>
        </section>
      </div>
    </div>
  </main>
</template>
